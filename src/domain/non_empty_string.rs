use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct NonEmptyString(pub(crate) String);
