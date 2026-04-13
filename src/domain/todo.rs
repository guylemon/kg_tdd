use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct Todo;
