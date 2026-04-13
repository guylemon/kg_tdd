use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::any::TypeId;
use std::sync::{Arc, Mutex, mpsc};
use text_splitter::{ChunkConfig, TextSplitter};

use crate::adapters::TokenizerSource;
use crate::application::{AppError, Chunk, ExtractionOutcome, IngestConfig, MaxConcurrency};
use crate::domain::{
    AnnotatedText, EdgeDescription, EntityMention, EntityName, EntityType, EpistemicStatus, Fact,
    FactualClaim, NodeDescription, NodeId, NonEmptyString, RelationshipMention, RelationshipType,
    TextUnit, Todo, TokenCount,
};
use crate::ports::{ChunkExtractor, DocumentPartitioner};
use tokenizers::Tokenizer;

pub(crate) trait SchemaLlmClient: Send + Sync {
    fn generate_with_schema<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<T, Todo>
    where
        T: DeserializeOwned + JsonSchema + 'static;
}

impl SchemaLlmClient for Todo {
    fn generate_with_schema<T>(
        &self,
        _sys_prompt: NonEmptyString,
        _user_prompt: NonEmptyString,
    ) -> Result<T, Todo>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        let schema = schemars::schema_for!(T);
        let schema_json = serde_json::to_string_pretty(&schema).map_err(|_| Todo)?;
        serde_json::from_str(&schema_json).map_err(|_| Todo)
    }
}

pub(crate) struct FakeSchemaLlmClient;

impl SchemaLlmClient for FakeSchemaLlmClient {
    fn generate_with_schema<T>(
        &self,
        _sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<T, Todo>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        let payload = if TypeId::of::<T>() == TypeId::of::<AiExtractionResponse>() {
            fixture_entities(&user_prompt.0)
        } else if TypeId::of::<T>() == TypeId::of::<AiRelationshipExtractionResponse>() {
            fixture_relationships(&user_prompt.0)
        } else {
            return Err(Todo);
        };

        serde_json::from_value(payload).map_err(|_| Todo)
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
        .generate_with_schema::<AiRelationshipExtractionResponse>(sys_prompt, user_prompt)
        .map_err(|_| AppError::ExtractChunk)?;

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
    llm_client
        .generate_with_schema::<AiExtractionResponse>(sys_prompt, user_prompt)
        .map_err(|_| AppError::ExtractChunk)
}

fn token_count(tokenizer: &Tokenizer, text: &str) -> Result<TokenCount, AppError> {
    let encoding = tokenizer
        .encode(text, false)
        .map_err(|_| AppError::ExtractChunk)?;
    Ok(TokenCount(encoding.len()))
}

#[cfg(test)]
mod tests {
    use super::{FakeSchemaLlmClient, ParallelChunkExtractor, extract_chunk};
    use crate::adapters::StaticTokenizerSource;
    use crate::application::{Chunk, IngestConfig, MaxConcurrency};
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
                    "missing-tokenizer",
                ))
            }
        }

        let result = ParallelChunkExtractor::new(
            IngestConfig {
                tokenizer_name: String::from("missing-tokenizer"),
                max_chunk_tokens: 8,
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
