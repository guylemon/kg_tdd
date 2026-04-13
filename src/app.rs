use crate::adapters::{ParallelChunkExtractor, SchemaLlmClient, TokenizerSource};
use crate::application::{AppError, IngestDocumentService, RunConfig};
use crate::ports::{DocumentSource, GraphArtifactSink};

pub struct App<S, G, C, T> {
    config: RunConfig,
    document_source: S,
    graph_sink: G,
    llm_client: C,
    tokenizer_source: T,
}

impl<S, G, C, T> App<S, G, C, T>
where
    S: DocumentSource,
    G: GraphArtifactSink,
    C: SchemaLlmClient + 'static,
    T: TokenizerSource,
{
    pub fn new(
        config: RunConfig,
        document_source: S,
        graph_sink: G,
        llm_client: C,
        tokenizer_source: T,
    ) -> Self {
        Self {
            config,
            document_source,
            graph_sink,
            llm_client,
            tokenizer_source,
        }
    }

    pub fn run(self) -> Result<(), AppError> {
        let extractor = ParallelChunkExtractor::new(
            self.config.ingest,
            self.config.max_concurrency,
            self.llm_client,
            &self.tokenizer_source,
        )?;
        let service = IngestDocumentService::new(extractor);

        let document = self
            .document_source
            .read_document(&self.config.input_path)?;
        let knowledge_graph = service.execute(&document)?;
        self.graph_sink
            .write_graph(&self.config.output_dir, &knowledge_graph)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use tokenizers::Tokenizer;
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;

    use super::App;
    use crate::adapters::{FakeSchemaLlmClient, FileGraphArtifactSink, StaticTokenizerSource};
    use crate::application::{
        AppError, IngestConfig, MaxConcurrency, ProviderConfig, RunConfig,
    };
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
            RunConfig {
                ingest: config,
                input_path: PathBuf::from("fixtures/input.txt"),
                output_dir: PathBuf::from("out"),
                max_concurrency: MaxConcurrency(1),
                provider: ProviderConfig::default(),
            },
            StubDocumentSource {
                document: Document {
                    id: DocumentId(String::from("doc-1")),
                    text: NonEmptyString(String::from(
                        "An apple is a red fruit that grows on trees",
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
        assert_eq!(writes[0].0, PathBuf::from("out"));
        assert!(writes[0].1.nodes.iter().any(|node| node.name.0 == "apple"));
        assert!(writes[0].1.nodes.iter().any(|node| node.name.0 == "fruit"));
        assert!(writes[0].1.nodes.iter().any(|node| node.name.0 == "trees"));
        assert!(
            writes[0]
                .1
                .edges
                .iter()
                .any(|edge| matches!(edge.edge_type, crate::domain::RelationshipType::IsA))
        );
        assert!(
            writes[0]
                .1
                .edges
                .iter()
                .any(|edge| matches!(edge.edge_type, crate::domain::RelationshipType::GrowsOn))
        );
    }

    #[test]
    fn runs_end_to_end_for_multi_chunk_document() {
        let config = IngestConfig {
            tokenizer_name: String::from("test-wordlevel"),
            max_chunk_tokens: 12,
        };
        let sink = RecordingGraphSink::default();
        let app = App::new(
            RunConfig {
                ingest: config,
                input_path: PathBuf::from("fixtures/input.txt"),
                output_dir: PathBuf::from("out"),
                max_concurrency: MaxConcurrency(2),
                provider: ProviderConfig::default(),
            },
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

    #[test]
    fn writes_full_artifact_bundle_with_real_sink() {
        let output_dir = temp_dir("artifact_bundle");
        let config = IngestConfig {
            tokenizer_name: String::from("test-wordlevel"),
            max_chunk_tokens: 32,
        };
        let app = App::new(
            RunConfig {
                ingest: config,
                input_path: PathBuf::from("fixtures/input.txt"),
                output_dir: output_dir.clone(),
                max_concurrency: MaxConcurrency(1),
                provider: ProviderConfig::default(),
            },
            StubDocumentSource {
                document: Document {
                    id: DocumentId(String::from("doc-1")),
                    text: NonEmptyString(String::from(
                        "An apple is a red fruit that grows on trees",
                    )),
                },
            },
            FileGraphArtifactSink,
            FakeSchemaLlmClient,
            StaticTokenizerSource::new(build_test_tokenizer()),
        );

        app.run().expect("app run succeeds");

        assert!(output_dir.join("graph.json").is_file());
        assert!(output_dir.join("index.html").is_file());
        assert!(output_dir.join("cytoscape.min.js").is_file());
        assert!(
            fs::read_to_string(output_dir.join("index.html"))
                .expect("index html")
                .contains("./graph.json")
        );
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

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("kg_tdd_{label}_{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
