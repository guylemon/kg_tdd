use serde::Serialize;

/// Newtype to prevent rogue string use
#[derive(Debug, Serialize)]
pub(crate) struct EdgeDescription(pub(crate) String);
