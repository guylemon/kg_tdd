use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct TokenCount(pub(crate) usize);
