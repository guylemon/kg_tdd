use crate::domain::{
    DocumentId, EntityMention, NonEmptyString, RelationshipMention, TokenCount,
};

// TODO remove public field and create initializer
#[derive(Clone, Copy)]
pub(crate) struct MaxConcurrency(pub u8);

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
