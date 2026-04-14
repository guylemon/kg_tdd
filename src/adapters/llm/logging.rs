use log::debug;

use super::schema::ChatCompletionRequest;

pub(super) const PROVIDER_RESPONSE_SNIPPET_LEN: usize = 240;
const RAW_PROVIDER_DEBUG_ENV: &str = "KG_DEBUG_RAW_PROVIDER";

pub(super) fn raw_provider_debug_enabled() -> bool {
    matches!(
        std::env::var(RAW_PROVIDER_DEBUG_ENV).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

pub(super) fn log_provider_request(
    schema_name: &str,
    endpoint: &str,
    model: &str,
    body: &ChatCompletionRequest,
    request_json: &str,
) {
    if raw_provider_debug_enabled() {
        debug!(
            "raw provider request for schema={schema_name}, endpoint={endpoint}, model={model}: {request_json}"
        );
        return;
    }

    let system_prompt_len = body
        .messages
        .iter()
        .find(|message| message.role == "system")
        .map_or(0, |message| message.content.len());
    let user_prompt_len = body
        .messages
        .iter()
        .find(|message| message.role == "user")
        .map_or(0, |message| message.content.len());

    debug!(
        "provider request summary: schema={schema_name}, endpoint={endpoint}, model={model}, request_bytes={}, system_prompt_len={}, user_prompt_len={}",
        request_json.len(),
        system_prompt_len,
        user_prompt_len
    );
}

pub(super) fn log_provider_response(schema_name: &str, endpoint: &str, response_text: &str) {
    if raw_provider_debug_enabled() {
        debug!(
            "raw provider response for schema={schema_name}, endpoint={endpoint}: {response_text}"
        );
        return;
    }

    debug!(
        "provider response summary: schema={schema_name}, endpoint={endpoint}, response_bytes={}, snippet={}",
        response_text.len(),
        snippet(response_text, PROVIDER_RESPONSE_SNIPPET_LEN)
    );
}

pub(super) fn snippet(value: &str, max_len: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_len {
        return value.to_owned();
    }

    let prefix: String = value.chars().take(max_len).collect();
    format!("{prefix}...")
}
