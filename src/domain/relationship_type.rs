use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub(crate) enum RelationshipType {
    GrowsOn,
    IsA,
}
