use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::application::AppError;
use crate::domain::{EntityType, NonEmptyString, RelationshipType};

const PROVIDER_MAX_TOKENS: u16 = 4096;

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
    json_schema: JsonSchemaResponseFormat,
}

#[derive(Serialize)]
struct JsonSchemaResponseFormat {
    name: String,
    strict: bool,
    schema: Value,
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
    let raw_schema = serde_json::to_value(schemars::schema_for!(T))
        .map_err(|_| AppError::provider_response("failed to serialize request schema"))?;
    Ok(normalize_schema_for_llm(raw_schema))
}

fn normalize_schema_for_llm(mut schema: Value) -> Value {
    normalize_string_const_enums(&mut schema);

    let definitions = collect_local_definitions(&schema);
    if !definitions.is_empty() {
        inline_local_refs(&mut schema, &definitions);
    }

    remove_non_provider_schema_keywords(&mut schema);

    schema
}

fn remove_non_provider_schema_keywords(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("$defs");
            object.remove("$schema");
            object.remove("title");

            for child in object.values_mut() {
                remove_non_provider_schema_keywords(child);
            }
        }
        Value::Array(array) => {
            for child in array {
                remove_non_provider_schema_keywords(child);
            }
        }
        _ => {}
    }
}

fn normalize_string_const_enums(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                normalize_string_const_enums(child);
            }

            if let Some(normalized_enum) = string_const_enum_schema(object) {
                *value = normalized_enum;
            }
        }
        Value::Array(array) => {
            for child in array {
                normalize_string_const_enums(child);
            }
        }
        _ => {}
    }
}

fn string_const_enum_schema(object: &Map<String, Value>) -> Option<Value> {
    let variants = object.get("oneOf")?.as_array()?;
    if variants.is_empty() {
        return None;
    }

    let mut enum_values = Vec::with_capacity(variants.len());
    for variant in variants {
        let variant_object = variant.as_object()?;
        if variant_object.get("type") != Some(&Value::String(String::from("string"))) {
            return None;
        }

        match variant_object.get("const") {
            Some(Value::String(name)) => enum_values.push(Value::String(name.clone())),
            _ => return None,
        }
    }

    let mut normalized = Map::new();
    normalized.insert(String::from("type"), Value::String(String::from("string")));
    normalized.insert(String::from("enum"), Value::Array(enum_values));
    Some(Value::Object(normalized))
}

fn collect_local_definitions(schema: &Value) -> Vec<(String, Value)> {
    let Some(definitions) = schema.get("$defs").and_then(Value::as_object) else {
        return Vec::new();
    };

    definitions
        .iter()
        .map(|(name, definition)| (format!("#/$defs/{name}"), definition.clone()))
        .collect()
}

fn inline_local_refs(value: &mut Value, definitions: &[(String, Value)]) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                if object.len() == 1 {
                    if let Some((_, definition)) = definitions
                        .iter()
                        .find(|(candidate, _)| candidate == reference)
                    {
                        *value = definition.clone();
                        inline_local_refs(value, definitions);
                        return;
                    }
                }
            }

            for child in object.values_mut() {
                inline_local_refs(child, definitions);
            }
        }
        Value::Array(array) => {
            for child in array {
                inline_local_refs(child, definitions);
            }
        }
        _ => {}
    }
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
    let schema_name = std::any::type_name::<T>()
        .rsplit("::")
        .next()
        .unwrap_or("Response")
        .to_owned();
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
            kind: String::from("json_schema"),
            json_schema: JsonSchemaResponseFormat {
                name: schema_name,
                strict: true,
                schema,
            },
        },
        temperature: 0.0,
        max_tokens: PROVIDER_MAX_TOKENS,
    })
}
