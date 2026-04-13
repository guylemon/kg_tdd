use std::path::PathBuf;

use crate::domain::{DocumentId, EntityMention, NonEmptyString, RelationshipMention, TokenCount};

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
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            tokenizer_name: String::from("bert-base-cased"),
            max_chunk_tokens: 128,
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
}
