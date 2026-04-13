use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DocumentId(pub(crate) String);
