use std::path::PathBuf;

use crate::adapters::{
    ParallelChunkExtractor, SchemaLlmClient, TokenizerSource,
};
use crate::application::{AppError, IngestConfig, IngestDocumentService, MaxConcurrency};
use crate::ports::{DocumentSource, GraphArtifactSink};

pub struct App<S, G, C, T> {
    config: IngestConfig,
    input_path: PathBuf,
    output_dir: PathBuf,
    document_source: S,
    graph_sink: G,
    llm_client: C,
    tokenizer_source: T,
    max_concurrency: MaxConcurrency,
}

impl<S, G, C, T> App<S, G, C, T>
where
    S: DocumentSource,
    G: GraphArtifactSink,
    C: SchemaLlmClient + 'static,
    T: TokenizerSource,
{
    pub fn new(
        config: IngestConfig,
        input_path: PathBuf,
        output_dir: PathBuf,
        max_concurrency: MaxConcurrency,
        document_source: S,
        graph_sink: G,
        llm_client: C,
        tokenizer_source: T,
    ) -> Self {
        Self {
            config,
            input_path,
            output_dir,
            document_source,
            graph_sink,
            llm_client,
            tokenizer_source,
            max_concurrency,
        }
    }

    pub fn run(self) -> Result<(), AppError> {
        let extractor = ParallelChunkExtractor::new(
            self.config,
            self.max_concurrency,
            self.llm_client,
            &self.tokenizer_source,
        )?;
        let service = IngestDocumentService::new(extractor);

        let document = self.document_source.read_document(&self.input_path)?;
        let knowledge_graph = service.execute(&document)?;
        self.graph_sink
            .write_graph(&self.output_dir, &knowledge_graph)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use tokenizers::Tokenizer;
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;

    use super::App;
    use crate::adapters::{FakeSchemaLlmClient, StaticTokenizerSource};
    use crate::application::{AppError, IngestConfig, MaxConcurrency};
    use crate::domain::{Document, DocumentId, KnowledgeGraph, NonEmptyString};
    use crate::ports::{DocumentSource, GraphArtifactSink};

    struct StubDocumentSource {
        document: Document,
    }

    impl DocumentSource for StubDocumentSource {
        fn read_document(&self, input_path: &Path) -> Result<Document, AppError> {
            assert_eq!(input_path, Path::new("fixtures/input.txt"));
            Ok(Document {
                id: self.document.id.clone(),
                text: self.document.text.clone(),
            })
        }
    }

    #[derive(Default)]
    struct RecordingGraphSink {
        writes: std::sync::Mutex<Vec<(PathBuf, KnowledgeGraph)>>,
    }

    impl GraphArtifactSink for &RecordingGraphSink {
        fn write_graph(&self, output_dir: &Path, graph: &KnowledgeGraph) -> Result<(), AppError> {
            self.writes
                .lock()
                .expect("lock")
                .push((output_dir.to_path_buf(), graph.clone()));
            Ok(())
        }
    }

    #[test]
    fn runs_end_to_end_for_seed_document() {
        let config = IngestConfig {
            tokenizer_name: String::from("test-wordlevel"),
            max_chunk_tokens: 32,
        };
        let sink = RecordingGraphSink::default();
        let app = App::new(
            config,
            PathBuf::from("fixtures/input.txt"),
            PathBuf::from("out"),
            MaxConcurrency(1),
            StubDocumentSource {
                document: Document {
                    id: DocumentId(String::from("doc-1")),
                    text: NonEmptyString(String::from("An apple is a red fruit that grows on trees")),
                },
            },
            &sink,
            FakeSchemaLlmClient,
            StaticTokenizerSource::new(build_test_tokenizer()),
        );

        app.run().expect("app run succeeds");

        let writes = sink.writes.lock().expect("lock");
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, PathBuf::from("out"));
        assert!(writes[0].1.nodes.iter().any(|node| node.name.0 == "apple"));
        assert!(writes[0].1.nodes.iter().any(|node| node.name.0 == "fruit"));
        assert!(writes[0].1.nodes.iter().any(|node| node.name.0 == "trees"));
        assert!(writes[0].1.edges.iter().any(|edge| matches!(edge.edge_type, crate::domain::RelationshipType::IsA)));
        assert!(writes[0].1.edges.iter().any(|edge| matches!(edge.edge_type, crate::domain::RelationshipType::GrowsOn)));
    }

    #[test]
    fn runs_end_to_end_for_multi_chunk_document() {
        let config = IngestConfig {
            tokenizer_name: String::from("test-wordlevel"),
            max_chunk_tokens: 12,
        };
        let sink = RecordingGraphSink::default();
        let app = App::new(
            config,
            PathBuf::from("fixtures/input.txt"),
            PathBuf::from("out"),
            MaxConcurrency(2),
            StubDocumentSource {
                document: Document {
                    id: DocumentId(String::from("doc-1")),
                    text: NonEmptyString(String::from(
                        "An apple is a red fruit that grows on trees.\n\nAn apple is a red fruit that grows on trees.",
                    )),
                },
            },
            &sink,
            FakeSchemaLlmClient,
            StaticTokenizerSource::new(build_test_tokenizer()),
        );

        app.run().expect("app run succeeds");

        let writes = sink.writes.lock().expect("lock");
        assert_eq!(writes.len(), 1);
        assert!(writes[0].1.edges.iter().any(|edge| edge.weight.0 == 2));
    }

    fn build_test_tokenizer() -> Tokenizer {
        let vocab = [
            ("[UNK]".to_owned(), 0),
            ("an".to_owned(), 1),
            ("apple".to_owned(), 2),
            ("is".to_owned(), 3),
            ("a".to_owned(), 4),
            ("red".to_owned(), 5),
            ("fruit".to_owned(), 6),
            ("that".to_owned(), 7),
            ("grows".to_owned(), 8),
            ("on".to_owned(), 9),
            ("trees".to_owned(), 10),
            (".".to_owned(), 11),
        ]
        .into_iter()
        .collect();

        let model = WordLevel::builder()
            .vocab(vocab)
            .unk_token("[UNK]".into())
            .build()
            .expect("word level model");
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace));
        tokenizer
    }
}
