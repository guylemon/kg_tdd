use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub struct Todo;
