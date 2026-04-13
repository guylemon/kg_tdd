use serde::Serialize;

/// Represents a single chunk of a source document.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Fact(pub(crate) String);
