use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use text_splitter::{ChunkConfig, TextSplitter};

use crate::adapters::TokenizerSource;
use crate::application::{
    AppError, Chunk, ExtractionOutcome, IngestConfig, MaxConcurrency, ProviderConfig, ProviderMode,
};
use crate::domain::{
    AnnotatedText, EdgeDescription, EntityMention, EntityName, EntityType, EpistemicStatus, Fact,
    FactualClaim, NodeDescription, NodeId, NonEmptyString, RelationshipMention, RelationshipType,
    TextUnit, TokenCount,
};
use crate::ports::{ChunkExtractor, DocumentPartitioner};
use tokenizers::Tokenizer;
use ureq::Agent;

const PROVIDER_TIMEOUT_SECS: u64 = 30;
const PROVIDER_MAX_RETRIES: usize = 2;
const PROVIDER_MAX_TOKENS: u16 = 512;

pub(crate) trait SchemaLlmClient: Send + Sync {
    fn generate_with_schema<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<T, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static;
}

pub(crate) enum ConfiguredSchemaLlmClient {
    Fixture(FakeSchemaLlmClient),
    OpenAiCompatible(OpenAiCompatibleSchemaLlmClient),
}

impl ConfiguredSchemaLlmClient {
    pub(crate) fn from_config(config: &ProviderConfig) -> Result<Self, AppError> {
        match config.mode {
            ProviderMode::Fixture => Ok(Self::Fixture(FakeSchemaLlmClient)),
            ProviderMode::OpenAiCompatible => Ok(Self::OpenAiCompatible(
                OpenAiCompatibleSchemaLlmClient::from_config(config)?,
            )),
        }
    }
}

impl SchemaLlmClient for ConfiguredSchemaLlmClient {
    fn generate_with_schema<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<T, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        match self {
            Self::Fixture(client) => client.generate_with_schema(sys_prompt, user_prompt),
            Self::OpenAiCompatible(client) => client.generate_with_schema(sys_prompt, user_prompt),
        }
    }
}

pub(crate) struct FakeSchemaLlmClient;

impl SchemaLlmClient for FakeSchemaLlmClient {
    fn generate_with_schema<T>(
        &self,
        _sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<T, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        let payload =
            if std::any::TypeId::of::<T>() == std::any::TypeId::of::<AiExtractionResponse>() {
                fixture_entities(&user_prompt.0)
            } else if std::any::TypeId::of::<T>()
                == std::any::TypeId::of::<AiRelationshipExtractionResponse>()
            {
                fixture_relationships(&user_prompt.0)
            } else {
                return Err(AppError::provider_response(
                    "unsupported fixture schema requested",
                ));
            };

        serde_json::from_value(payload)
            .map_err(|_| AppError::provider_response("fixture response does not match schema"))
    }
}

pub(crate) struct OpenAiCompatibleSchemaLlmClient {
    agent: Agent,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl OpenAiCompatibleSchemaLlmClient {
    fn from_config(config: &ProviderConfig) -> Result<Self, AppError> {
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

        Ok(Self {
            agent: agent_config.into(),
            base_url: trim_trailing_slash(base_url),
            model,
            api_key,
        })
    }

    fn call_once<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<T, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        let endpoint = format!("{}/v1/chat/completions", self.base_url);
        let body = build_chat_request::<T>(&self.model, sys_prompt, user_prompt)?;

        let mut request = self
            .agent
            .post(&endpoint)
            .header("Content-Type", "application/json");
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", &format!("Bearer {api_key}"));
        }

        let mut response = match request.send_json(&body) {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(code)) => {
                return Err(classify_status_code(code, endpoint.as_str()));
            }
            Err(error) => return Err(classify_transport_error(error, endpoint.as_str())),
        };

        let response_body: ChatCompletionResponse = response
            .body_mut()
            .read_json()
            .map_err(|_| AppError::provider_response("response body is not valid JSON"))?;

        let content = response_body
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .ok_or_else(|| {
                AppError::provider_response("assistant message content missing from response")
            })?;

        serde_json::from_str(&content)
            .map_err(|_| AppError::provider_response("assistant content does not match schema"))
    }
}

impl SchemaLlmClient for OpenAiCompatibleSchemaLlmClient {
    fn generate_with_schema<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<T, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        retry_operation(PROVIDER_MAX_RETRIES, || {
            self.call_once::<T>(sys_prompt.clone(), user_prompt.clone())
        })
    }
}

pub(crate) struct ParallelChunkExtractor<C> {
    config: IngestConfig,
    max_concurrency: MaxConcurrency,
    llm_client: Arc<C>,
    tokenizer: Tokenizer,
}

impl<C> ParallelChunkExtractor<C>
where
    C: SchemaLlmClient + 'static,
{
    pub(crate) fn new<T>(
        config: IngestConfig,
        max_concurrency: MaxConcurrency,
        llm_client: C,
        tokenizer_source: &T,
    ) -> Result<Self, AppError>
    where
        T: TokenizerSource,
    {
        let tokenizer = tokenizer_source.load(&config.tokenizer_name)?;

        Ok(Self {
            config,
            max_concurrency,
            llm_client: Arc::new(llm_client),
            tokenizer,
        })
    }
}

impl<C> DocumentPartitioner for ParallelChunkExtractor<C>
where
    C: SchemaLlmClient + 'static,
{
    fn partition(&self, document: &crate::domain::Document) -> Result<Vec<Chunk>, AppError> {
        let config =
            ChunkConfig::new(self.config.max_chunk_tokens).with_sizer(self.tokenizer.clone());
        let splitter = TextSplitter::new(config);

        splitter
            .chunks(&document.text.0)
            .map(|text| {
                let text = text.to_owned();
                let token_count = token_count(&self.tokenizer, &text)?;
                Ok(Chunk {
                    document_id: document.id.clone(),
                    text: NonEmptyString(text),
                    token_count,
                })
            })
            .collect()
    }
}

impl<C> ChunkExtractor for ParallelChunkExtractor<C>
where
    C: SchemaLlmClient + 'static,
{
    fn extract(&self, chunk: Chunk) -> Result<ExtractionOutcome, AppError> {
        map_reduce_extract(&self.llm_client, self.max_concurrency, vec![chunk])?
            .into_iter()
            .next()
            .ok_or(AppError::ExtractChunk)
    }
}

#[derive(Debug)]
struct ExtractionTask {
    chunk: Chunk,
}

#[derive(Clone, Deserialize, JsonSchema, Serialize)]
struct RelationshipExtract {
    source: ExtractedEntity,
    target: ExtractedEntity,
    relationship_type: RelationshipType,
    description: String,
    fact: String,
}

#[derive(Clone, Deserialize, JsonSchema, Serialize)]
struct AiRelationshipExtractionResponse {
    relationships: Vec<RelationshipExtract>,
}

#[derive(Clone, Deserialize, JsonSchema, Serialize)]
struct ExtractedEntity {
    name: String,
    entity_type: EntityType,
    description: String,
}

#[derive(Clone, Deserialize, JsonSchema, Serialize)]
struct AiExtractionResponse {
    entities: Vec<ExtractedEntity>,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    response_format: ResponseFormat,
    temperature: f32,
    max_tokens: u16,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: String,
    schema: serde_json::Value,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessage,
}

#[derive(Deserialize)]
struct ChatCompletionMessage {
    content: Option<String>,
}

fn map_reduce_extract<C>(
    llm_client: &Arc<C>,
    max_concurrency: MaxConcurrency,
    chunks: Vec<Chunk>,
) -> Result<Vec<ExtractionOutcome>, AppError>
where
    C: SchemaLlmClient + 'static,
{
    let num_chunks = chunks.len();

    let (task_tx, task_rx) = mpsc::channel::<ExtractionTask>();
    let task_rx = Arc::new(Mutex::new(task_rx));
    let (result_tx, result_rx) = mpsc::channel::<Result<ExtractionOutcome, AppError>>();

    let mut handles = Vec::with_capacity(max_concurrency.0.into());

    for _ in 0..max_concurrency.0 {
        let task_receiver = Arc::clone(&task_rx);
        let result_transmitter = result_tx.clone();
        let llm = Arc::clone(llm_client);

        let handle = std::thread::spawn(move || {
            loop {
                let task = {
                    let guard = task_receiver.lock().expect("task receiver lock");
                    guard.recv()
                };

                match task {
                    Ok(task) => {
                        let extraction = extract_chunk(task.chunk, llm.as_ref());
                        let _ = result_transmitter.send(extraction);
                    }
                    Err(_) => break,
                }
            }
        });

        handles.push(handle);
    }

    for chunk in chunks {
        task_tx
            .send(ExtractionTask { chunk })
            .map_err(|_| AppError::ExtractChunk)?;
    }
    drop(task_tx);
    drop(result_tx);

    let mut results = Vec::with_capacity(num_chunks);
    for _ in 0..num_chunks {
        results.push(result_rx.recv().map_err(|_| AppError::ExtractChunk)??);
    }

    for handle in handles {
        handle.join().map_err(|_| AppError::ExtractChunk)?;
    }

    Ok(results)
}

fn extract_chunk<C>(chunk: Chunk, llm_client: &C) -> Result<ExtractionOutcome, AppError>
where
    C: SchemaLlmClient + 'static,
{
    let entity_response = request_entity_extraction(&chunk.text, llm_client)?;
    let marked = mark_entities(chunk, &entity_response);
    let entities = extract_entities(&marked, entity_response);
    let relationships = extract_relationships(&marked, llm_client)?;

    Ok(ExtractionOutcome {
        entities,
        relationships,
    })
}

fn extract_relationships<C>(
    text_unit: &TextUnit,
    llm_client: &C,
) -> Result<Vec<RelationshipMention>, AppError>
where
    C: SchemaLlmClient + 'static,
{
    let sys_prompt = NonEmptyString(String::from("Extract entity relationships from the text."));
    let user_prompt = NonEmptyString(text_unit.text.0.clone());
    let response = llm_client
        .generate_with_schema::<AiRelationshipExtractionResponse>(sys_prompt, user_prompt)?;

    Ok(response
        .relationships
        .into_iter()
        .map(|relationship| RelationshipMention {
            source: entity_name_to_node_id(&relationship.source.name),
            target: entity_name_to_node_id(&relationship.target.name),
            description: EdgeDescription(relationship.description),
            evidence: vec![FactualClaim {
                fact: Fact(relationship.fact),
                citation: text_unit.clone(),
                status: EpistemicStatus::Probable,
            }],
            relationship_type: relationship.relationship_type,
        })
        .collect())
}

fn extract_entities(text_unit: &TextUnit, response: AiExtractionResponse) -> Vec<EntityMention> {
    response
        .entities
        .into_iter()
        .map(|entity| EntityMention {
            description: NodeDescription(entity.description),
            entity_type: entity.entity_type,
            name: EntityName(entity.name),
            source: text_unit.clone(),
        })
        .collect()
}

fn mark_entities(chunk: Chunk, response: &AiExtractionResponse) -> TextUnit {
    let Chunk {
        document_id,
        text,
        token_count,
    } = chunk;
    let text = annotate_text(text.0, response);

    TextUnit {
        document_id,
        text: AnnotatedText(text),
        token_count,
    }
}

fn annotate_text(text: String, response: &AiExtractionResponse) -> String {
    let mut result = text;
    let mut entities = response.entities.clone();
    entities.sort_by(|left, right| right.name.len().cmp(&left.name.len()));

    for entity in entities {
        let annotation = format!(
            "<entity type=\"{:?}\">{}</entity>",
            entity.entity_type, entity.name
        );
        result = result.replace(&entity.name, &annotation);
    }

    result
}

fn entity_name_to_node_id(name: &str) -> NodeId {
    NodeId(format!("node:{}", slugify(name)))
}

fn slugify(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() {
                char
            } else {
                '-'
            }
        })
        .collect()
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
                    "fact": "An apple is a red fruit"
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
                    "fact": "apple grows on trees"
                }
            ]
        })
    } else {
        serde_json::json!({ "relationships": [] })
    }
}

fn request_entity_extraction<C>(
    text: &NonEmptyString,
    llm_client: &C,
) -> Result<AiExtractionResponse, AppError>
where
    C: SchemaLlmClient + 'static,
{
    let sys_prompt = NonEmptyString(String::from("Mark entities in the text."));
    let user_prompt = text.clone();
    llm_client.generate_with_schema::<AiExtractionResponse>(sys_prompt, user_prompt)
}

fn token_count(tokenizer: &Tokenizer, text: &str) -> Result<TokenCount, AppError> {
    let encoding = tokenizer
        .encode(text, false)
        .map_err(|_| AppError::ExtractChunk)?;
    Ok(TokenCount(encoding.len()))
}

fn schema_for<T>() -> Result<serde_json::Value, AppError>
where
    T: JsonSchema,
{
    serde_json::to_value(schemars::schema_for!(T))
        .map_err(|_| AppError::provider_response("failed to serialize request schema"))
}

fn build_chat_request<T>(
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

fn trim_trailing_slash(url: String) -> String {
    url.trim_end_matches('/').to_owned()
}

fn validate_base_url(base_url: &str) -> Result<(), AppError> {
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
            AppError::provider_timeout(format!("{} while calling {}", timeout, endpoint))
        }
        ureq::Error::StatusCode(code) => classify_status_code(code, endpoint),
        other => AppError::provider_transport(format!("{other} while calling {endpoint}")),
    }
}

fn classify_status_code(code: u16, endpoint: &str) -> AppError {
    let message = format!("HTTP {code} from {endpoint}");
    if code == 401 || code == 403 {
        AppError::provider_authentication(message)
    } else if (500..=599).contains(&code) {
        AppError::provider_transport(message)
    } else {
        AppError::provider_response(message)
    }
}

fn is_retryable(error: &AppError) -> bool {
    matches!(
        error,
        AppError::ProviderTransport(_) | AppError::ProviderTimeout(_)
    )
}

fn retry_operation<T, F>(max_retries: usize, mut operation: F) -> Result<T, AppError>
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
    use super::{
        ConfiguredSchemaLlmClient, FakeSchemaLlmClient, ParallelChunkExtractor, build_chat_request,
        classify_status_code, extract_chunk, retry_operation, validate_base_url,
    };
    use crate::adapters::StaticTokenizerSource;
    use crate::application::{
        AppError, Chunk, IngestConfig, MaxConcurrency, ProviderConfig, ProviderMode,
    };
    use crate::domain::{Document, DocumentId, NonEmptyString};
    use crate::ports::DocumentPartitioner;
    use tokenizers::Tokenizer;
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;

    #[test]
    fn partitions_document_into_single_chunk() {
        let extractor = ParallelChunkExtractor::new(
            IngestConfig {
                tokenizer_name: String::from("test-wordlevel"),
                max_chunk_tokens: 32,
            },
            MaxConcurrency(1),
            FakeSchemaLlmClient,
            &StaticTokenizerSource::new(build_test_tokenizer()),
        )
        .expect("extractor");
        let document = Document {
            id: DocumentId(String::from("stdin-document")),
            text: NonEmptyString(String::from("An apple is a red fruit that grows on trees")),
        };

        let chunks = extractor.partition(&document).expect("chunks");

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].document_id.0, "stdin-document");
        assert_eq!(chunks[0].token_count.0, 10);
    }

    #[test]
    fn fake_client_extracts_fixture_entities_and_relationships() {
        let outcome = extract_chunk(
            Chunk {
                document_id: DocumentId(String::from("stdin-document")),
                text: NonEmptyString(String::from("An apple is a red fruit that grows on trees")),
                token_count: crate::domain::TokenCount(10),
            },
            &FakeSchemaLlmClient,
        )
        .expect("outcome");

        assert_eq!(outcome.entities.len(), 3);
        assert_eq!(outcome.relationships.len(), 2);
        assert_eq!(outcome.relationships[0].source.0, "node:apple");
        assert_eq!(outcome.relationships[1].target.0, "node:trees");
    }

    #[test]
    fn partitions_long_document_into_multiple_chunks() {
        let extractor = ParallelChunkExtractor::new(
            IngestConfig {
                tokenizer_name: String::from("test-wordlevel"),
                max_chunk_tokens: 12,
            },
            MaxConcurrency(1),
            FakeSchemaLlmClient,
            &StaticTokenizerSource::new(build_test_tokenizer()),
        )
        .expect("extractor");
        let document = Document {
            id: DocumentId(String::from("stdin-document")),
            text: NonEmptyString(String::from(
                "An apple is a red fruit that grows on trees.\n\nAn apple is a red fruit that grows on trees.",
            )),
        };

        let chunks = extractor.partition(&document).expect("chunks");

        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|chunk| chunk.token_count.0 <= 12));
        assert_eq!(
            chunks[0].text.0,
            "An apple is a red fruit that grows on trees."
        );
        assert_eq!(
            chunks[1].text.0,
            "An apple is a red fruit that grows on trees."
        );
    }

    #[test]
    fn fails_fast_when_tokenizer_cannot_be_loaded() {
        struct BrokenTokenizerSource;

        impl crate::adapters::TokenizerSource for BrokenTokenizerSource {
            fn load(
                &self,
                _tokenizer_name: &str,
            ) -> Result<Tokenizer, crate::application::AppError> {
                Err(crate::application::AppError::load_tokenizer(
                    "test-wordlevel",
                ))
            }
        }

        let result = ParallelChunkExtractor::new(
            IngestConfig {
                tokenizer_name: String::from("test-wordlevel"),
                max_chunk_tokens: 32,
            },
            MaxConcurrency(1),
            FakeSchemaLlmClient,
            &BrokenTokenizerSource,
        );

        assert!(matches!(
            result,
            Err(crate::application::AppError::LoadTokenizer(_))
        ));
    }

    #[test]
    fn validates_provider_base_url() {
        validate_base_url("http://localhost:8080").expect("valid URL");
        assert!(validate_base_url("localhost:8080").is_err());
    }

    #[test]
    fn configured_client_uses_fixture_mode_by_default() {
        let client = ConfiguredSchemaLlmClient::from_config(&ProviderConfig {
            mode: ProviderMode::Fixture,
            base_url: None,
            model: None,
        })
        .expect("fixture client");

        let outcome = extract_chunk(
            Chunk {
                document_id: DocumentId(String::from("stdin-document")),
                text: NonEmptyString(String::from("An apple is a red fruit that grows on trees")),
                token_count: crate::domain::TokenCount(10),
            },
            &client,
        )
        .expect("extracts");

        assert_eq!(outcome.entities.len(), 3);
    }

    #[test]
    fn openai_compatible_client_sends_schema_constrained_request() {
        let request = build_chat_request::<super::AiExtractionResponse>(
            "llama3.2",
            NonEmptyString(String::from("system prompt")),
            NonEmptyString(String::from("user prompt")),
        )
        .expect("request");
        let request = serde_json::to_value(request).expect("request json");

        assert_eq!(request["model"], "llama3.2");
        assert_eq!(request["messages"][0]["role"], "system");
        assert_eq!(request["messages"][1]["role"], "user");
        assert_eq!(request["response_format"]["type"], "json_object");
        assert!(request["response_format"]["schema"].is_object());
        assert_eq!(request["temperature"], 0.0);
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
    fn openai_compatible_client_does_not_retry_auth_failures() {
        let mut attempts = 0;
        let result = retry_operation::<(), _>(2, || {
            attempts += 1;
            Err(AppError::provider_authentication("forbidden"))
        });

        assert!(matches!(result, Err(AppError::ProviderAuthentication(_))));
        assert_eq!(attempts, 1);
    }

    fn build_test_tokenizer() -> Tokenizer {
        let vocab = [
            ("[UNK]".to_owned(), 0),
            ("an".to_owned(), 1),
            ("apple".to_owned(), 2),
            ("is".to_owned(), 3),
            ("a".to_owned(), 4),
            ("red".to_owned(), 5),
            ("fruit".to_owned(), 6),
            ("that".to_owned(), 7),
            ("grows".to_owned(), 8),
            ("on".to_owned(), 9),
            ("trees".to_owned(), 10),
            (".".to_owned(), 11),
        ]
        .into_iter()
        .collect();

        let model = WordLevel::builder()
            .vocab(vocab)
            .unk_token("[UNK]".into())
            .build()
            .expect("word level model");
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace));
        tokenizer
    }
}
