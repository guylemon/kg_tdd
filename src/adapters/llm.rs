use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::sync::{mpsc, Arc, Mutex};

use crate::application::{AppError, Chunk, ExtractionOutcome, MaxConcurrency};
use crate::domain::{
    AnnotatedText, EdgeDescription, EntityMention, NonEmptyString, RelationshipMention, TextUnit,
    Todo,
};
use crate::ports::{ChunkExtractor, DocumentPartitioner};

pub(crate) trait SchemaLlmClient: Send + Sync {
    fn generate_with_schema<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<T, Todo>
    where
        T: DeserializeOwned + JsonSchema;
}

impl SchemaLlmClient for Todo {
    fn generate_with_schema<T>(
        &self,
        _sys_prompt: NonEmptyString,
        _user_prompt: NonEmptyString,
    ) -> Result<T, Todo>
    where
        T: DeserializeOwned + JsonSchema,
    {
        let schema = schemars::schema_for!(T);
        let schema_json = serde_json::to_string_pretty(&schema).map_err(|_| Todo)?;
        serde_json::from_str(&schema_json).map_err(|_| Todo)
    }
}

pub(crate) struct ParallelChunkExtractor<C> {
    max_concurrency: MaxConcurrency,
    llm_client: Arc<C>,
}

impl<C> ParallelChunkExtractor<C>
where
    C: SchemaLlmClient + 'static,
{
    pub(crate) fn new(max_concurrency: MaxConcurrency, llm_client: C) -> Self {
        Self {
            max_concurrency,
            llm_client: Arc::new(llm_client),
        }
    }
}

impl<C> DocumentPartitioner for ParallelChunkExtractor<C>
where
    C: SchemaLlmClient + 'static,
{
    fn partition(&self, _document: &crate::domain::Document) -> Result<Vec<Chunk>, AppError> {
        Ok(Vec::new())
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

#[derive(Deserialize, JsonSchema, Serialize)]
struct RelationshipExtract {
    relation: Todo,
    source: String,
    target: String,
    relationship_type: Todo,
    description: String,
}

#[derive(Deserialize, JsonSchema, Serialize)]
struct AiRelationshipExtractionResponse {
    relationships: Vec<RelationshipExtract>,
}

#[derive(Deserialize, JsonSchema, Serialize)]
struct AiExtractionResponse {
    entities: Vec<String>,
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
    let marked = mark_entities(chunk, llm_client)?;
    let entities = extract_entities(&marked);
    let relationships = extract_relationships(marked, llm_client)?;

    Ok(ExtractionOutcome {
        entities,
        relationships,
    })
}

fn extract_relationships<C>(
    _text_unit: TextUnit,
    llm_client: &C,
) -> Result<Vec<RelationshipMention>, AppError>
where
    C: SchemaLlmClient + 'static,
{
    let relationship_types: Vec<Todo> = Vec::new();
    let mut extracted_relationships: Vec<RelationshipExtract> = Vec::new();

    for _relationship_type in &relationship_types {
        let sys_prompt = NonEmptyString(String::new());
        let user_prompt = NonEmptyString(String::new());
        let response = llm_client
            .generate_with_schema::<AiRelationshipExtractionResponse>(sys_prompt, user_prompt)
            .map_err(|_| AppError::ExtractChunk)?;

        extracted_relationships.extend(response.relationships);
    }

    Ok(extracted_relationships
        .into_iter()
        .map(|relationship| RelationshipMention {
            source: crate::domain::NodeId(relationship.source),
            target: crate::domain::NodeId(relationship.target),
            description: EdgeDescription(relationship.description),
            evidence: Vec::new(),
            relationship_type: relationship.relationship_type,
        })
        .collect())
}

fn extract_entities(_text_unit: &TextUnit) -> Vec<EntityMention> {
    Vec::new()
}

fn mark_entities<C>(chunk: Chunk, llm_client: &C) -> Result<TextUnit, AppError>
where
    C: SchemaLlmClient + 'static,
{
    let mut text = chunk.text.0;
    let supported_types: Vec<Todo> = Vec::new();

    for _entity_type in &supported_types {
        let sys_prompt = NonEmptyString(String::new());
        let user_prompt = NonEmptyString(String::new());
        let response = llm_client
            .generate_with_schema::<AiExtractionResponse>(sys_prompt, user_prompt)
            .map_err(|_| AppError::ExtractChunk)?;

        text = annotate_text(text, response);
    }

    Ok(TextUnit {
        document_id: chunk.document_id,
        text: AnnotatedText(text),
        token_count: chunk.token_count,
    })
}

fn annotate_text(text: String, response: AiExtractionResponse) -> String {
    let mut result = text;
    for entity in response.entities {
        result = entity;
    }
    result
}
