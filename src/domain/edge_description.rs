use serde::Serialize;

/// Newtype to prevent rogue string use
#[derive(Clone, Debug, Serialize)]
pub(crate) struct EdgeDescription(pub(crate) String);
