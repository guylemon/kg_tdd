use serde::Serialize;

use super::AnnotatedText;
use super::DocumentId;
use super::TokenCount;

/// Represents a single chunk of a source document.
#[derive(Debug, Serialize)]
pub(crate) struct TextUnit {
    /// The id of the source document
    pub(crate) document_id: DocumentId,

    /// The raw chunk text
    pub(crate) text: AnnotatedText,

    /// The number of tokens in the raw chunk text
    /// ``token_count`` can be used to control context size in LLM prompts containing the
    /// ``TextUnit``
    pub(crate) token_count: TokenCount,
}
