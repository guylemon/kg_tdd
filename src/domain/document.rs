use super::DocumentId;
use super::NonEmptyString;

/// Represents a unit of source material for the graph produced by this application
#[allow(unused)]
pub(crate) struct Document {
    // TODO define how this is derived
    pub(crate) id: DocumentId,

    /// The full raw text of the document
    pub(crate) text: NonEmptyString,
}
