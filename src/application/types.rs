use std::path::PathBuf;

use serde::Serialize;

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
}

#[derive(Clone, Debug)]
pub(crate) struct RunConfig {
    pub(crate) ingest: IngestConfig,
    pub(crate) input_path: PathBuf,
    pub(crate) output_dir: PathBuf,
    pub(crate) max_concurrency: MaxConcurrency,
    pub(crate) provider: ProviderConfig,
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            tokenizer_name: String::from("bert-base-cased"),
            max_chunk_tokens: 128,
        }
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
