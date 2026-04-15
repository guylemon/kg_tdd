use std::path::{Path, PathBuf};

use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::application::error::AppError;
use crate::domain::{
    DocumentId, EntityMention, KnowledgeGraph, NonEmptyString, RelationshipMention, TokenCount,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProviderMode {
    Fixture,
    OpenAiCompatible,
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderConfig {
    pub(crate) mode: ProviderMode,
    pub(crate) base_url: Option<String>,
    pub(crate) model: Option<String>,
}

// TODO remove public field and create initializer
#[derive(Clone, Copy, Debug)]
pub(crate) struct MaxConcurrency(pub u8);

#[derive(Clone, Debug)]
pub(crate) struct IngestConfig {
    pub(crate) tokenizer_name: String,
    pub(crate) max_chunk_tokens: usize,
    pub(crate) prompt_templates_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct RunConfig {
    pub(crate) ingest: IngestConfig,
    pub(crate) input_path: PathBuf,
    pub(crate) output_dir: PathBuf,
    pub(crate) max_concurrency: MaxConcurrency,
    pub(crate) provider: ProviderConfig,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RunMode {
    Cli,
    GoldEval,
}

#[derive(Clone, Debug)]
pub(crate) struct RunContext {
    pub(crate) run_id: String,
    pub(crate) started_at: String,
    pub(crate) mode: RunMode,
    pub(crate) input_path: PathBuf,
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) provider: ProviderMetadata,
    pub(crate) tokenizer_name: String,
    pub(crate) max_chunk_tokens: usize,
    pub(crate) max_concurrency: u8,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct RunCounts {
    #[serde(rename = "chunk_count")]
    pub(crate) chunks: Option<usize>,
    #[serde(rename = "extracted_entity_count")]
    pub(crate) extracted_entities: Option<usize>,
    #[serde(rename = "extracted_relationship_count")]
    pub(crate) extracted_relationships: Option<usize>,
    #[serde(rename = "final_node_count")]
    pub(crate) final_nodes: Option<usize>,
    #[serde(rename = "final_edge_count")]
    pub(crate) final_edges: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RunMetadata {
    pub(crate) run_id: String,
    pub(crate) started_at: String,
    pub(crate) finished_at: String,
    pub(crate) status: RunStatus,
    pub(crate) mode: RunMode,
    pub(crate) input_path: PathBuf,
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) debug_dir: Option<PathBuf>,
    pub(crate) document_id: Option<String>,
    pub(crate) provider: ProviderMetadata,
    pub(crate) tokenizer_name: String,
    pub(crate) max_chunk_tokens: usize,
    pub(crate) max_concurrency: u8,
    pub(crate) counts: RunCounts,
    pub(crate) error: Option<RunErrorMetadata>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RunStatus {
    Success,
    Failure,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ProviderMetadata {
    pub(crate) mode: String,
    pub(crate) model: Option<String>,
    pub(crate) base_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RunErrorMetadata {
    pub(crate) category: String,
    pub(crate) message: String,
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            tokenizer_name: String::from("bert-base-cased"),
            max_chunk_tokens: 128,
            prompt_templates_dir: Self::default_prompt_templates_dir(),
        }
    }
}

impl IngestConfig {
    pub(crate) fn default_prompt_templates_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("prompts")
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            mode: ProviderMode::Fixture,
            base_url: None,
            model: None,
        }
    }
}

impl RunContext {
    pub(crate) fn new<P, O>(
        mode: RunMode,
        input_path: P,
        output_dir: Option<O>,
        provider: &ProviderConfig,
        ingest: &IngestConfig,
        max_concurrency: MaxConcurrency,
    ) -> Self
    where
        P: Into<PathBuf>,
        O: Into<PathBuf>,
    {
        let timestamp = OffsetDateTime::now_utc();
        let started_at = format_rfc3339(timestamp);
        let run_id = format!(
            "run-{}-p{}",
            timestamp.unix_timestamp_nanos(),
            std::process::id()
        );

        Self {
            run_id,
            started_at,
            mode,
            input_path: input_path.into(),
            output_dir: output_dir.map(Into::into),
            provider: ProviderMetadata::new(provider),
            tokenizer_name: ingest.tokenizer_name.clone(),
            max_chunk_tokens: ingest.max_chunk_tokens,
            max_concurrency: max_concurrency.0,
        }
    }

    pub(crate) fn finish(
        &self,
        document_id: Option<&DocumentId>,
        result: Option<&TraceableIngestResult>,
        finished_at: String,
        status: RunStatus,
        error: Option<RunErrorMetadata>,
    ) -> RunMetadata {
        let counts = result.map_or_else(RunCounts::default, RunCounts::from_result);

        self.finish_impl(document_id, finished_at, status, counts, error)
    }

    pub(crate) fn finish_with_trace(
        &self,
        document_id: Option<&DocumentId>,
        trace: Option<&IngestionTrace>,
        finished_at: String,
        status: RunStatus,
        error: Option<RunErrorMetadata>,
    ) -> RunMetadata {
        let counts = trace.map_or_else(RunCounts::default, RunCounts::from_trace);

        self.finish_impl(document_id, finished_at, status, counts, error)
    }

    pub(crate) fn with_output_dir<P>(&self, output_dir: P) -> Self
    where
        P: Into<PathBuf>,
    {
        let mut context = self.clone();
        context.output_dir = Some(output_dir.into());
        context
    }

    fn finish_impl(
        &self,
        document_id: Option<&DocumentId>,
        finished_at: String,
        status: RunStatus,
        counts: RunCounts,
        error: Option<RunErrorMetadata>,
    ) -> RunMetadata {
        RunMetadata {
            run_id: self.run_id.clone(),
            started_at: self.started_at.clone(),
            finished_at,
            status,
            mode: self.mode.clone(),
            input_path: self.input_path.clone(),
            output_dir: self.output_dir.clone(),
            debug_dir: self.output_dir.as_ref().map(|dir| dir.join("debug")),
            document_id: document_id.map(|id| id.0.clone()),
            provider: self.provider.clone(),
            tokenizer_name: self.tokenizer_name.clone(),
            max_chunk_tokens: self.max_chunk_tokens,
            max_concurrency: self.max_concurrency,
            counts,
            error,
        }
    }
}

impl RunCounts {
    pub(crate) fn from_result(result: &TraceableIngestResult) -> Self {
        Self {
            chunks: Some(result.trace.chunks.len()),
            extracted_entities: Some(
                result
                    .trace
                    .extracted_mentions
                    .iter()
                    .map(|chunk| chunk.entities.len())
                    .sum::<usize>(),
            ),
            extracted_relationships: Some(
                result
                    .trace
                    .extracted_mentions
                    .iter()
                    .map(|chunk| chunk.relationships.len())
                    .sum::<usize>(),
            ),
            final_nodes: Some(result.graph.nodes.len()),
            final_edges: Some(result.graph.edges.len()),
        }
    }

    pub(crate) fn from_trace(trace: &IngestionTrace) -> Self {
        let extracted_entities = trace
            .extracted_mentions
            .iter()
            .map(|chunk| chunk.entities.len())
            .sum::<usize>();
        let extracted_relationships = trace
            .extracted_mentions
            .iter()
            .map(|chunk| chunk.relationships.len())
            .sum::<usize>();

        Self {
            chunks: Some(trace.chunks.len()),
            extracted_entities: Some(extracted_entities),
            extracted_relationships: Some(extracted_relationships),
            final_nodes: None,
            final_edges: None,
        }
    }
}

impl ProviderMetadata {
    fn new(provider: &ProviderConfig) -> Self {
        Self {
            mode: provider_mode_label(&provider.mode).to_owned(),
            model: provider.model.clone(),
            base_url: provider.base_url.clone(),
        }
    }
}

impl RunErrorMetadata {
    pub(crate) fn new(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            message: message.into(),
        }
    }
}

impl RunMode {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::GoldEval => "gold-eval",
        }
    }
}

pub(crate) fn utc_now_rfc3339() -> String {
    format_rfc3339(OffsetDateTime::now_utc())
}

fn format_rfc3339(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&Rfc3339)
        .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"))
}

fn provider_mode_label(mode: &ProviderMode) -> &'static str {
    match mode {
        ProviderMode::Fixture => "fixture",
        ProviderMode::OpenAiCompatible => "openai-compatible",
    }
}

/// A unit of raw work in the ingestion pipeline prior to entity marking.
#[derive(Clone, Debug)]
pub(crate) struct Chunk {
    pub(crate) document_id: DocumentId,
    pub(crate) text: NonEmptyString,
    pub(crate) token_count: TokenCount,
}

pub(crate) struct ExtractionOutcome {
    pub(crate) entities: Vec<EntityMention>,
    pub(crate) relationships: Vec<RelationshipMention>,
    pub(crate) provider_responses: Vec<CapturedProviderResponse>,
}

#[derive(Clone)]
pub(crate) struct TraceableIngestResult {
    pub(crate) graph: KnowledgeGraph,
    pub(crate) trace: IngestionTrace,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct IngestionTrace {
    pub(crate) chunks: Vec<ChunkTrace>,
    pub(crate) provider_responses: Vec<ProviderResponseTrace>,
    pub(crate) extracted_mentions: Vec<ChunkExtractionTrace>,
}

#[derive(Debug)]
pub(crate) struct TraceableIngestError {
    pub(crate) error: AppError,
    pub(crate) trace: IngestionTrace,
}

impl TraceableIngestError {
    pub(crate) fn new(error: AppError, trace: IngestionTrace) -> Self {
        Self { error, trace }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ChunkTrace {
    pub(crate) index: usize,
    pub(crate) document_id: String,
    pub(crate) text: String,
    pub(crate) token_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum ProviderResponseKind {
    EntityExtraction,
    RelationshipExtraction,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ProviderResponseTrace {
    pub(crate) chunk_index: usize,
    pub(crate) kind: ProviderResponseKind,
    pub(crate) raw_response: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct CapturedProviderResponse {
    pub(crate) kind: ProviderResponseKind,
    pub(crate) raw_response: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ChunkExtractionTrace {
    pub(crate) chunk_index: usize,
    pub(crate) entities: Vec<EntityMentionTrace>,
    pub(crate) relationships: Vec<RelationshipMentionTrace>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct EntityMentionTrace {
    pub(crate) name: String,
    pub(crate) entity_type: String,
    pub(crate) description: String,
    pub(crate) source_document_id: String,
    pub(crate) source_text: String,
    pub(crate) source_token_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct RelationshipMentionTrace {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) relationship_type: String,
    pub(crate) description: String,
    pub(crate) evidence: Vec<EvidenceTrace>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct EvidenceTrace {
    pub(crate) fact: String,
    pub(crate) citation_text: String,
    pub(crate) status: String,
    pub(crate) source_document_id: String,
    pub(crate) source_text: String,
    pub(crate) source_token_count: usize,
}
