use std::any::TypeId;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use tracing::debug;

use crate::application::AppError;
use crate::domain::NonEmptyString;

use super::client::{GeneratedSchemaValue, SchemaLlmClient};
use super::schema::{AiExtractionResponse, AiRelationshipExtractionResponse};

pub(crate) struct FakeSchemaLlmClient;

impl SchemaLlmClient for FakeSchemaLlmClient {
    fn generate_with_schema<T>(
        &self,
        _sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<GeneratedSchemaValue<T>, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        debug!(
            "using fixture schema client for {} with user_prompt_len={}",
            std::any::type_name::<T>(),
            user_prompt.0.len()
        );
        let payload = if TypeId::of::<T>() == TypeId::of::<AiExtractionResponse>() {
            fixture_entities(&user_prompt.0)
        } else if TypeId::of::<T>() == TypeId::of::<AiRelationshipExtractionResponse>() {
            fixture_relationships(&user_prompt.0)
        } else {
            return Err(AppError::provider_response(
                "unsupported fixture schema requested",
            ));
        };

        let raw_response = payload.to_string();
        let parsed = serde_json::from_value(payload)
            .map_err(|_| AppError::provider_response("fixture response does not match schema"))?;

        Ok(GeneratedSchemaValue {
            parsed,
            raw_response,
        })
    }
}

fn fixture_entities(user_prompt: &str) -> serde_json::Value {
    if user_prompt.contains("apple")
        && user_prompt.contains("fruit")
        && user_prompt.contains("trees")
    {
        serde_json::json!({
            "entities": [
                {
                    "name": "apple",
                    "entity_type": "Lifeform",
                    "description": "A red fruit"
                },
                {
                    "name": "fruit",
                    "entity_type": "Concept",
                    "description": "An edible plant structure"
                },
                {
                    "name": "trees",
                    "entity_type": "Lifeform",
                    "description": "Woody plants"
                }
            ]
        })
    } else {
        serde_json::json!({ "entities": [] })
    }
}

fn fixture_relationships(user_prompt: &str) -> serde_json::Value {
    if user_prompt.contains("apple")
        && user_prompt.contains("fruit")
        && user_prompt.contains("trees")
    {
        serde_json::json!({
            "relationships": [
                {
                    "source": {
                        "name": "apple",
                        "entity_type": "Lifeform",
                        "description": "A red fruit"
                    },
                    "target": {
                        "name": "fruit",
                        "entity_type": "Concept",
                        "description": "An edible plant structure"
                    },
                    "relationship_type": "IsA",
                    "description": "apple is a fruit",
                    "evidence": [
                        {
                            "fact": "An apple is a red fruit",
                            "citation_text": "apple is a red fruit"
                        }
                    ]
                },
                {
                    "source": {
                        "name": "apple",
                        "entity_type": "Lifeform",
                        "description": "A red fruit"
                    },
                    "target": {
                        "name": "trees",
                        "entity_type": "Lifeform",
                        "description": "Woody plants"
                    },
                    "relationship_type": "GrowsOn",
                    "description": "apple grows on trees",
                    "evidence": [
                        {
                            "fact": "apple grows on trees",
                            "citation_text": "grows on trees"
                        }
                    ]
                }
            ]
        })
    } else {
        serde_json::json!({ "relationships": [] })
    }
}
