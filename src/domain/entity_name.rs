use serde::Serialize;

/// Newtype to prevent rogue string use
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct EntityName(pub(crate) String);
