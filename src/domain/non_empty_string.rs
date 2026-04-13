use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct NonEmptyString(pub(crate) String);
