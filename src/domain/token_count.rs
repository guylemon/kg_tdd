use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct TokenCount(pub(crate) usize);
