use tracing::debug;

use crate::adapters::{ParallelChunkExtractor, SchemaLlmClient, TokenizerSource};
use crate::application::{
    AppError, IngestDocumentService, RunConfig, RunContext, RunMode, RunStatus, utc_now_rfc3339,
};
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
        let Self {
            config,
            document_source,
            graph_sink,
            llm_client,
            tokenizer_source,
        } = self;

        let run_context = build_run_context(&config);
        log_run_started(&config, &run_context);

        let extractor = build_extractor(
            config.ingest.clone(),
            config.max_concurrency,
            llm_client,
            &tokenizer_source,
        )?;
        let service = IngestDocumentService::new(extractor);
        let document = load_document(&config, &document_source, &run_context)?;
        let ingest_result = execute_ingestion(&run_context, &service, &document)?;
        let metadata = finish_run(&run_context, &document, &ingest_result);
        write_artifacts(
            &config,
            &graph_sink,
            &run_context,
            &document,
            &ingest_result,
            &metadata,
        )?;
        log_artifacts_written(&config, &metadata);

        Ok(())
    }
}

fn build_run_context(config: &RunConfig) -> RunContext {
    RunContext::new(
        RunMode::Cli,
        config.input_path.clone(),
        Some(config.output_dir.clone()),
        &config.provider,
        &config.ingest,
        config.max_concurrency,
    )
}

fn build_extractor<C, T>(
    config: crate::application::IngestConfig,
    max_concurrency: crate::application::MaxConcurrency,
    llm_client: C,
    tokenizer_source: &T,
) -> Result<ParallelChunkExtractor<C>, AppError>
where
    C: SchemaLlmClient + 'static,
    T: TokenizerSource,
{
    ParallelChunkExtractor::new(config, max_concurrency, llm_client, tokenizer_source)
}

fn load_document<S>(
    config: &RunConfig,
    document_source: &S,
    run_context: &RunContext,
) -> Result<crate::domain::Document, AppError>
where
    S: DocumentSource,
{
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        input_path = %config.input_path.display(),
        "document load started"
    );
    let document = match document_source.read_document(&config.input_path) {
        Ok(document) => document,
        Err(err) => {
            log_run_failed(run_context, None, &err);
            return Err(err);
        }
    };
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        document_id = %document.id.0,
        document_bytes = document.text.0.len(),
        "document loaded"
    );
    Ok(document)
}

fn execute_ingestion<C>(
    run_context: &RunContext,
    service: &IngestDocumentService<ParallelChunkExtractor<C>>,
    document: &crate::domain::Document,
) -> Result<crate::application::TraceableIngestResult, AppError>
where
    C: SchemaLlmClient + 'static,
{
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        document_id = %document.id.0,
        "ingestion started"
    );
    let ingest_result = match service.execute_with_trace(document) {
        Ok(result) => result,
        Err(err) => {
            log_run_failed(run_context, Some(&document.id), &err);
            return Err(err);
        }
    };
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        document_id = %document.id.0,
        chunks = ingest_result.trace.chunks.len(),
        extracted_entities = ingest_result
            .trace
            .extracted_mentions
            .iter()
            .map(|chunk| chunk.entities.len())
            .sum::<usize>(),
        extracted_relationships = ingest_result
            .trace
            .extracted_mentions
            .iter()
            .map(|chunk| chunk.relationships.len())
            .sum::<usize>(),
        final_nodes = ingest_result.graph.nodes.len(),
        final_edges = ingest_result.graph.edges.len(),
        "ingestion completed"
    );
    Ok(ingest_result)
}

fn finish_run(
    run_context: &RunContext,
    document: &crate::domain::Document,
    ingest_result: &crate::application::TraceableIngestResult,
) -> crate::application::RunMetadata {
    run_context.finish(
        Some(&document.id),
        Some(ingest_result),
        utc_now_rfc3339(),
        RunStatus::Success,
        None,
    )
}

fn write_artifacts<G>(
    config: &RunConfig,
    graph_sink: &G,
    run_context: &RunContext,
    document: &crate::domain::Document,
    ingest_result: &crate::application::TraceableIngestResult,
    metadata: &crate::application::RunMetadata,
) -> Result<(), AppError>
where
    G: GraphArtifactSink,
{
    if let Err(err) = graph_sink.write_graph(&config.output_dir, &ingest_result.graph) {
        log_run_failed(run_context, Some(&document.id), &err);
        return Err(err);
    }

    if let Err(err) =
        graph_sink.write_debug_artifacts(&config.output_dir, &ingest_result.trace, metadata)
    {
        log_run_failed(run_context, Some(&document.id), &err);
        return Err(err);
    }

    Ok(())
}

fn log_run_started(config: &RunConfig, run_context: &RunContext) {
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        input_path = %run_context.input_path.display(),
        output_dir = %config.output_dir.display(),
        provider_mode = %run_context.provider.mode,
        tokenizer_name = %run_context.tokenizer_name,
        max_chunk_tokens = run_context.max_chunk_tokens,
        max_concurrency = run_context.max_concurrency,
        "run started"
    );
}

fn log_artifacts_written(config: &RunConfig, metadata: &crate::application::RunMetadata) {
    debug!(
        run_id = %metadata.run_id,
        mode = %metadata.mode.label(),
        output_dir = %config.output_dir.display(),
        debug_dir = %config.output_dir.join("debug").display(),
        "artifacts written"
    );
}

fn log_run_failed(
    run_context: &RunContext,
    document_id: Option<&crate::domain::DocumentId>,
    err: &AppError,
) {
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        input_path = %run_context.input_path.display(),
        output_dir = %run_context
            .output_dir
            .as_ref()
            .map_or_else(|| String::from("<none>"), |path| path.display().to_string()),
        document_id = %document_id
            .map_or_else(|| String::from("<none>"), |id| id.0.clone()),
        error_category = %err.metadata_category(),
        error = %err,
        "run failed"
    );
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
        AppError, IngestConfig, IngestionTrace, MaxConcurrency, ProviderConfig, RunConfig,
        RunMetadata, RunStatus,
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
        debug_writes: std::sync::Mutex<Vec<(PathBuf, IngestionTrace, RunMetadata)>>,
    }

    impl GraphArtifactSink for &RecordingGraphSink {
        fn write_graph(&self, output_dir: &Path, graph: &KnowledgeGraph) -> Result<(), AppError> {
            self.writes
                .lock()
                .expect("lock")
                .push((output_dir.to_path_buf(), graph.clone()));
            Ok(())
        }

        fn write_debug_artifacts(
            &self,
            output_dir: &Path,
            trace: &IngestionTrace,
            metadata: &RunMetadata,
        ) -> Result<(), AppError> {
            self.debug_writes.lock().expect("lock").push((
                output_dir.to_path_buf(),
                trace.clone(),
                metadata.clone(),
            ));
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
        let debug_writes = sink.debug_writes.lock().expect("lock");
        assert_eq!(debug_writes.len(), 1);
        assert_eq!(debug_writes[0].0, PathBuf::from("out"));
        assert_eq!(debug_writes[0].1.chunks.len(), 1);
        assert_eq!(debug_writes[0].1.provider_responses.len(), 2);
        assert!(matches!(debug_writes[0].2.status, RunStatus::Success));
        assert_eq!(debug_writes[0].2.mode.label(), "cli");
        assert_eq!(debug_writes[0].2.output_dir, Some(PathBuf::from("out")));
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
        let debug_writes = sink.debug_writes.lock().expect("lock");
        assert_eq!(debug_writes.len(), 1);
        assert_eq!(debug_writes[0].1.chunks.len(), 2);
        assert_eq!(debug_writes[0].1.provider_responses.len(), 4);
        assert_eq!(debug_writes[0].2.counts.chunks, Some(2));
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
        assert!(output_dir.join("debug").join("chunk-list.json").is_file());
        assert!(
            output_dir
                .join("debug")
                .join("raw-provider-responses.json")
                .is_file()
        );
        assert!(
            output_dir
                .join("debug")
                .join("extracted-mentions.json")
                .is_file()
        );
        assert!(output_dir.join("debug").join("run-metadata.json").is_file());
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
