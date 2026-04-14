use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::application::AppError;
use crate::domain::{EntityType, NonEmptyString, RelationshipType};

const PROVIDER_MAX_TOKENS: u16 = 512;

#[derive(Clone, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RelationshipExtract {
    pub(crate) source: ExtractedEntity,
    pub(crate) target: ExtractedEntity,
    pub(crate) relationship_type: RelationshipType,
    pub(crate) description: String,
    pub(crate) evidence: Vec<ExtractedEvidence>,
}

#[derive(Clone, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiRelationshipExtractionResponse {
    pub(crate) relationships: Vec<RelationshipExtract>,
}

#[derive(Clone, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExtractedEvidence {
    pub(crate) fact: String,
    pub(crate) citation_text: String,
}

#[derive(Clone, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExtractedEntity {
    pub(crate) name: String,
    pub(crate) entity_type: EntityType,
    pub(crate) description: String,
}

#[derive(Clone, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiExtractionResponse {
    pub(crate) entities: Vec<ExtractedEntity>,
}

#[derive(Serialize)]
pub(super) struct ChatCompletionRequest {
    model: String,
    pub(super) messages: Vec<ChatMessage>,
    response_format: ResponseFormat,
    temperature: f32,
    max_tokens: u16,
}

#[derive(Serialize)]
pub(super) struct ChatMessage {
    pub(super) role: String,
    pub(super) content: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: String,
    schema: serde_json::Value,
}

#[derive(Deserialize)]
pub(super) struct ChatCompletionResponse {
    pub(super) choices: Vec<ChatCompletionChoice>,
}

#[derive(Deserialize)]
pub(super) struct ChatCompletionChoice {
    pub(super) message: ChatCompletionMessage,
}

#[derive(Deserialize)]
pub(super) struct ChatCompletionMessage {
    pub(super) content: Option<String>,
}

fn schema_for<T>() -> Result<serde_json::Value, AppError>
where
    T: JsonSchema,
{
    serde_json::to_value(schemars::schema_for!(T))
        .map_err(|_| AppError::provider_response("failed to serialize request schema"))
}

pub(super) fn build_chat_request<T>(
    model: &str,
    sys_prompt: NonEmptyString,
    user_prompt: NonEmptyString,
) -> Result<ChatCompletionRequest, AppError>
where
    T: JsonSchema,
{
    let schema = schema_for::<T>()?;
    Ok(ChatCompletionRequest {
        model: model.to_owned(),
        messages: vec![
            ChatMessage {
                role: String::from("system"),
                content: sys_prompt.0,
            },
            ChatMessage {
                role: String::from("user"),
                content: user_prompt.0,
            },
        ],
        response_format: ResponseFormat {
            kind: String::from("json_object"),
            schema,
        },
        temperature: 0.0,
        max_tokens: PROVIDER_MAX_TOKENS,
    })
}
