use std::time::Duration;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use tracing::debug;
use ureq::Agent;

use crate::application::{AppError, ProviderConfig};
use crate::domain::NonEmptyString;

use super::client::{GeneratedSchemaValue, SchemaLlmClient};
use super::logging::{
    PROVIDER_RESPONSE_SNIPPET_LEN, log_provider_request, log_provider_response,
    raw_provider_debug_enabled, snippet,
};
use super::schema::{ChatCompletionResponse, build_chat_request};

const PROVIDER_TIMEOUT_SECS: u64 = 30;
const PROVIDER_MAX_RETRIES: usize = 2;

pub(crate) struct OpenAiCompatibleSchemaLlmClient {
    agent: Agent,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl OpenAiCompatibleSchemaLlmClient {
    pub(crate) fn from_config(config: &ProviderConfig) -> Result<Self, AppError> {
        let base_url = config.base_url.clone().ok_or_else(|| {
            AppError::invalid_provider_config(
                "missing required flag for openai-compatible mode: --provider-base-url",
            )
        })?;
        validate_base_url(&base_url)?;

        let model = config.model.clone().ok_or_else(|| {
            AppError::invalid_provider_config(
                "missing required flag for openai-compatible mode: --provider-model",
            )
        })?;

        let agent_config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(PROVIDER_TIMEOUT_SECS)))
            .build();
        let api_key = std::env::var("KG_PROVIDER_API_KEY").ok();

        debug!(
            "configured openai-compatible client: base_url={}, model={}, has_api_key={}",
            base_url,
            model,
            api_key.is_some()
        );

        Ok(Self {
            agent: agent_config.into(),
            base_url: trim_trailing_slash(&base_url),
            model,
            api_key,
        })
    }

    fn call_once<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<GeneratedSchemaValue<T>, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        let endpoint = format!("{}/v1/chat/completions", self.base_url);
        let body = build_chat_request::<T>(&self.model, sys_prompt, user_prompt)?;
        let request_json = serde_json::to_string(&body).map_err(|_| {
            AppError::provider_response("failed to serialize provider request body")
        })?;
        let schema_name = std::any::type_name::<T>();

        log_provider_request(
            schema_name,
            endpoint.as_str(),
            &self.model,
            &body,
            &request_json,
        );

        let mut request = self
            .agent
            .post(&endpoint)
            .header("Content-Type", "application/json");
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", &format!("Bearer {api_key}"));
        }

        let mut response = match request.send_json(&body) {
            Ok(response) => {
                debug!("provider request succeeded: schema={schema_name}, endpoint={endpoint}");
                response
            }
            Err(ureq::Error::StatusCode(code)) => {
                debug!(
                    "provider request returned non-success status: schema={schema_name}, endpoint={endpoint}, status={code}"
                );
                return Err(classify_status_code(code, endpoint.as_str()));
            }
            Err(error) => {
                debug!(
                    "provider request failed before response parsing: schema={schema_name}, endpoint={endpoint}, error={error}"
                );
                return Err(classify_transport_error(error, endpoint.as_str()));
            }
        };

        let response_text = response
            .body_mut()
            .read_to_string()
            .map_err(|_| AppError::provider_response("response body is not valid UTF-8 text"))?;
        log_provider_response(schema_name, endpoint.as_str(), &response_text);

        let response_body: ChatCompletionResponse =
            serde_json::from_str(&response_text).map_err(|_| {
                debug!(
                    "provider response JSON parse failed: schema={}, endpoint={}, snippet={}",
                    schema_name,
                    endpoint,
                    snippet(&response_text, PROVIDER_RESPONSE_SNIPPET_LEN)
                );
                AppError::provider_response("response body is not valid JSON")
            })?;

        let content = assistant_message_content(response_body, schema_name, &endpoint)?;

        if raw_provider_debug_enabled() {
            debug!("raw provider assistant content for {schema_name}: {content}");
        } else {
            debug!(
                "provider assistant content summary for {}: len={}, snippet={}",
                schema_name,
                content.len(),
                snippet(&content, PROVIDER_RESPONSE_SNIPPET_LEN)
            );
        }

        let parsed = parse_assistant_content::<T>(&content, schema_name)?;

        Ok(GeneratedSchemaValue {
            parsed,
            raw_response: response_text,
        })
    }
}

impl SchemaLlmClient for OpenAiCompatibleSchemaLlmClient {
    fn generate_with_schema<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<GeneratedSchemaValue<T>, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        let schema_name = std::any::type_name::<T>();
        let mut attempts = 0;

        loop {
            attempts += 1;
            debug!(
                "provider attempt {attempts} for schema={schema_name} (max_retries={PROVIDER_MAX_RETRIES})"
            );

            match self.call_once::<T>(sys_prompt.clone(), user_prompt.clone()) {
                Ok(value) => return Ok(value),
                Err(err) if is_retryable(&err) && attempts <= PROVIDER_MAX_RETRIES => {
                    debug!("retrying provider request for schema={schema_name} after error={err}");
                }
                Err(err) => {
                    debug!(
                        "provider request failed for schema={schema_name} after {attempts} attempt(s): {err}"
                    );
                    return Err(err);
                }
            }
        }
    }
}

fn trim_trailing_slash(url: &str) -> String {
    url.trim_end_matches('/').to_owned()
}

pub(super) fn validate_base_url(base_url: &str) -> Result<(), AppError> {
    match ureq::http::Uri::try_from(base_url) {
        Ok(uri) if uri.scheme().is_some() && uri.host().is_some() => Ok(()),
        _ => Err(AppError::invalid_provider_config(format!(
            "provider base URL is invalid: {base_url}"
        ))),
    }
}

fn classify_transport_error(error: ureq::Error, endpoint: &str) -> AppError {
    match error {
        ureq::Error::Timeout(timeout) => {
            AppError::provider_timeout(format!("{timeout} while calling {endpoint}"))
        }
        ureq::Error::StatusCode(code) => classify_status_code(code, endpoint),
        other => AppError::provider_transport(format!("{other} while calling {endpoint}")),
    }
}

pub(super) fn classify_status_code(code: u16, endpoint: &str) -> AppError {
    let message = format!("HTTP {code} from {endpoint}");
    if code == 401 || code == 403 {
        AppError::provider_authentication(message)
    } else if code == 429 || (500..=599).contains(&code) {
        AppError::provider_transport(message)
    } else {
        AppError::provider_response(message)
    }
}

fn assistant_message_content(
    response_body: ChatCompletionResponse,
    schema_name: &str,
    endpoint: &str,
) -> Result<String, AppError> {
    response_body
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .ok_or_else(|| {
            debug!(
                "provider response missing assistant message content: schema={schema_name}, endpoint={endpoint}"
            );
            AppError::provider_response("assistant message content missing from response")
        })
}

fn parse_assistant_content<T>(content: &str, schema_name: &str) -> Result<T, AppError>
where
    T: DeserializeOwned + JsonSchema + 'static,
{
    serde_json::from_str(content).map_err(|_| {
        debug!(
            "provider assistant content schema mismatch: schema={}, snippet={}",
            schema_name,
            snippet(content, PROVIDER_RESPONSE_SNIPPET_LEN)
        );
        AppError::provider_response("assistant content does not match schema")
    })
}

fn is_retryable(error: &AppError) -> bool {
    matches!(
        error,
        AppError::ProviderTransport(_) | AppError::ProviderTimeout(_)
    )
}

#[cfg(test)]
pub(super) fn retry_operation<T, F>(max_retries: usize, mut operation: F) -> Result<T, AppError>
where
    F: FnMut() -> Result<T, AppError>,
{
    let mut attempts = 0;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(err) if is_retryable(&err) && attempts < max_retries => {
                attempts += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::application::AppError;
    use crate::application::{ProviderConfig, ProviderMode};
    use crate::domain::NonEmptyString;

    use super::{
        OpenAiCompatibleSchemaLlmClient, classify_status_code, parse_assistant_content,
        retry_operation, validate_base_url,
    };
    use crate::adapters::llm::schema::{
        AiExtractionResponse, AiRelationshipExtractionResponse, build_chat_request,
    };

    #[test]
    fn validates_provider_base_url() {
        validate_base_url("http://localhost:8080").expect("valid URL");
        assert!(validate_base_url("localhost:8080").is_err());
    }

    #[test]
    fn from_config_rejects_missing_base_url_in_openai_mode() {
        let config = ProviderConfig {
            mode: ProviderMode::OpenAiCompatible,
            base_url: None,
            model: Some(String::from("gpt-4o-mini")),
        };

        let result = OpenAiCompatibleSchemaLlmClient::from_config(&config);
        assert!(result.is_err(), "missing base url should fail");
        let err = result.err().expect("error expected");

        assert!(matches!(err, AppError::InvalidProviderConfig(_)));
        assert!(
            err.to_string()
                .contains("missing required flag for openai-compatible mode: --provider-base-url")
        );
    }

    #[test]
    fn from_config_rejects_missing_model_in_openai_mode() {
        let config = ProviderConfig {
            mode: ProviderMode::OpenAiCompatible,
            base_url: Some(String::from("http://localhost:8080")),
            model: None,
        };

        let result = OpenAiCompatibleSchemaLlmClient::from_config(&config);
        assert!(result.is_err(), "missing model should fail");
        let err = result.err().expect("error expected");

        assert!(matches!(err, AppError::InvalidProviderConfig(_)));
        assert!(
            err.to_string()
                .contains("missing required flag for openai-compatible mode: --provider-model")
        );
    }

    #[test]
    fn openai_compatible_client_sends_schema_constrained_request() {
        let request = build_chat_request::<AiExtractionResponse>(
            "llama3.2",
            NonEmptyString(String::from("system prompt")),
            NonEmptyString(String::from("user prompt")),
        )
        .expect("request");
        let request = serde_json::to_value(request).expect("request json");

        assert_eq!(request["model"], "llama3.2");
        assert_eq!(request["messages"][0]["role"], "system");
        assert_eq!(request["messages"][1]["role"], "user");
        assert_eq!(request["response_format"]["type"], "json_schema");
        assert_eq!(
            request["response_format"]["json_schema"]["name"],
            "AiExtractionResponse"
        );
        assert_eq!(request["response_format"]["json_schema"]["strict"], true);
        assert!(request["response_format"]["json_schema"]["schema"].is_object());
        assert_eq!(
            request["response_format"]["json_schema"]["schema"]["properties"]["entities"]["items"]
                ["properties"]["entity_type"]["type"],
            "string"
        );
        assert_eq!(
            request["response_format"]["json_schema"]["schema"]["properties"]["entities"]["items"]
                ["properties"]["entity_type"]["enum"],
            serde_json::json!([
                "Concept",
                "Event",
                "Lifeform",
                "Location",
                "Organization",
                "Person",
                "Product",
                "Technology"
            ])
        );
        assert_eq!(request["temperature"], 0.0);
    }

    #[test]
    fn relationship_schema_includes_closed_nested_evidence_items_and_string_enum_relationship_type()
    {
        let request = build_chat_request::<AiRelationshipExtractionResponse>(
            "llama3.2",
            NonEmptyString(String::from("system prompt")),
            NonEmptyString(String::from("user prompt")),
        )
        .expect("request");
        let request = serde_json::to_value(request).expect("request json");
        let schema = &request["response_format"]["json_schema"]["schema"];
        let schema_text = schema.to_string();

        assert!(schema_text.contains("\"citation_text\""));
        assert!(schema_text.contains("\"fact\""));
        assert!(schema_text.contains("\"additionalProperties\":false"));
        assert_eq!(
            schema["properties"]["relationships"]["items"]["properties"]["relationship_type"]["type"],
            "string"
        );
        assert_eq!(
            schema["properties"]["relationships"]["items"]["properties"]["relationship_type"]["enum"],
            serde_json::json!(["GrowsOn", "IsA"])
        );
        assert!(
            !schema_text.contains("\"const\":\"Person\""),
            "entity and relationship enums should be emitted as string enums"
        );
        assert!(
            !schema_text.contains("\"const\":\"GrowsOn\""),
            "entity and relationship enums should be emitted as string enums"
        );
    }

    #[test]
    fn malformed_provider_content_is_reported_as_provider_response() {
        let result = parse_assistant_content::<AiExtractionResponse>(
            "{ this is not valid json }",
            "ai::extract",
        );
        assert!(result.is_err(), "malformed content should fail");
        let err = result.err().expect("error expected");

        assert!(matches!(err, AppError::ProviderResponse(_)));
        assert!(
            err.to_string()
                .contains("assistant content does not match schema")
        );
    }

    #[test]
    fn status_classification_marks_auth_and_retryable_failures_correctly() {
        let endpoint = "http://localhost:8080/v1/chat/completions";

        assert!(matches!(
            classify_status_code(401, endpoint),
            AppError::ProviderAuthentication(_)
        ));
        assert!(matches!(
            classify_status_code(403, endpoint),
            AppError::ProviderAuthentication(_)
        ));
        assert!(matches!(
            classify_status_code(429, endpoint),
            AppError::ProviderTransport(_)
        ));
        assert!(matches!(
            classify_status_code(500, endpoint),
            AppError::ProviderTransport(_)
        ));
        assert!(matches!(
            classify_status_code(404, endpoint),
            AppError::ProviderResponse(_)
        ));
    }

    #[test]
    fn openai_compatible_client_retries_transient_failures() {
        let mut attempts = 0;
        let result = retry_operation::<(), _>(2, || {
            attempts += 1;
            if attempts == 1 {
                Err(classify_status_code(
                    500,
                    "http://localhost:8080/v1/chat/completions",
                ))
            } else {
                Ok(())
            }
        });

        assert!(result.is_ok());
        assert_eq!(attempts, 2);
    }

    #[test]
    fn openai_compatible_client_retries_rate_limit_failures() {
        let mut attempts = 0;
        let result = retry_operation::<(), _>(2, || {
            attempts += 1;
            if attempts == 1 {
                Err(classify_status_code(
                    429,
                    "http://localhost:8080/v1/chat/completions",
                ))
            } else {
                Ok(())
            }
        });

        assert!(result.is_ok());
        assert_eq!(attempts, 2);
    }

    #[test]
    fn openai_compatible_client_does_not_retry_auth_failures() {
        let mut attempts = 0;
        let result = retry_operation::<(), _>(2, || {
            attempts += 1;
            Err(AppError::provider_authentication("forbidden"))
        });

        assert!(matches!(result, Err(AppError::ProviderAuthentication(_))));
        assert_eq!(attempts, 1);
    }
}
