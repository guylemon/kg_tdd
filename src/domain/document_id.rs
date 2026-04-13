use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct DocumentId(pub(crate) String);
