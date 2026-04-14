use std::sync::{Arc, Mutex, mpsc};

use log::debug;
use text_splitter::{ChunkConfig, TextSplitter};
use tokenizers::Tokenizer;

use crate::application::{AppError, Chunk, ExtractionOutcome, IngestConfig, MaxConcurrency};
use crate::domain::{
    AnnotatedText, EdgeDescription, EntityMention, EntityName, FactualClaim, NodeDescription,
    NonEmptyString, RelationshipMention, TextUnit, TokenCount, node_id_for_entity,
};
use crate::ports::{ChunkExtractor, DocumentPartitioner};

use super::{
    TokenizerSource,
    llm::{
        AiExtractionResponse, AiRelationshipExtractionResponse, ExtractedEvidence, SchemaLlmClient,
    },
};

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

        let chunks: Vec<_> = splitter
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
            .collect::<Result<Vec<_>, AppError>>()?;

        debug!(
            "partitioned document {} into {} chunk(s)",
            document.id.0,
            chunks.len()
        );

        Ok(chunks)
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

pub(crate) fn extract_chunk<C>(chunk: Chunk, llm_client: &C) -> Result<ExtractionOutcome, AppError>
where
    C: SchemaLlmClient + 'static,
{
    debug!(
        "extracting chunk for document={} with token_count={}",
        chunk.document_id.0, chunk.token_count.0
    );
    let entity_response = request_entity_extraction(&chunk.text, llm_client)?;
    let marked = mark_entities(chunk, &entity_response);
    let entities = extract_entities(&marked, entity_response);
    let relationships = extract_relationships(&marked, llm_client)?;

    debug!(
        "finished chunk extraction: entities={}, relationships={}",
        entities.len(),
        relationships.len()
    );

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

    let source_text = deannotate_text(&text_unit.text.0);
    Ok(response
        .relationships
        .into_iter()
        .filter_map(|relationship| {
            let evidence = relationship
                .evidence
                .into_iter()
                .filter_map(|item| map_evidence_item(item, text_unit, &source_text))
                .collect::<Vec<_>>();

            if evidence.is_empty() {
                return None;
            }

            Some(RelationshipMention {
                source: node_id_for_entity(
                    &relationship.source.entity_type,
                    &EntityName(relationship.source.name),
                ),
                target: node_id_for_entity(
                    &relationship.target.entity_type,
                    &EntityName(relationship.target.name),
                ),
                description: EdgeDescription(relationship.description),
                evidence,
                relationship_type: relationship.relationship_type,
            })
        })
        .collect())
}

fn map_evidence_item(
    item: ExtractedEvidence,
    text_unit: &TextUnit,
    source_text: &str,
) -> Option<FactualClaim> {
    if !source_text.contains(&item.citation_text) {
        return None;
    }

    Some(FactualClaim::grounded(
        item.fact,
        item.citation_text,
        text_unit.clone(),
    ))
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

fn deannotate_text(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut in_tag = false;

    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => plain.push(ch),
            _ => {}
        }
    }

    plain
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

#[cfg(test)]
mod tests {
    use schemars::JsonSchema;
    use tokenizers::Tokenizer;
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;

    use crate::adapters::llm::AiExtractionResponse;
    use crate::adapters::{FakeSchemaLlmClient, SchemaLlmClient, StaticTokenizerSource};
    use crate::application::{AppError, Chunk, IngestConfig, MaxConcurrency};
    use crate::domain::{Document, DocumentId, NonEmptyString, TokenCount};
    use crate::ports::DocumentPartitioner;

    use super::{ParallelChunkExtractor, extract_chunk};

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
                token_count: TokenCount(10),
            },
            &FakeSchemaLlmClient,
        )
        .expect("outcome");

        assert_eq!(outcome.entities.len(), 3);
        assert_eq!(outcome.relationships.len(), 2);
        assert_eq!(outcome.relationships[0].source.0, "node:lifeform:apple");
        assert_eq!(outcome.relationships[1].target.0, "node:lifeform:trees");
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
    fn drops_invalid_evidence_items_and_keeps_relationship_with_valid_quote() {
        struct EvidenceClient;

        impl SchemaLlmClient for EvidenceClient {
            fn generate_with_schema<T>(
                &self,
                _sys_prompt: NonEmptyString,
                _user_prompt: NonEmptyString,
            ) -> Result<T, AppError>
            where
                T: serde::de::DeserializeOwned + JsonSchema + 'static,
            {
                let payload = if std::any::TypeId::of::<T>()
                    == std::any::TypeId::of::<AiExtractionResponse>()
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
                            }
                        ]
                    })
                } else {
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
                                    },
                                    {
                                        "fact": "Not grounded",
                                        "citation_text": "pear is blue"
                                    }
                                ]
                            }
                        ]
                    })
                };

                serde_json::from_value(payload)
                    .map_err(|_| AppError::provider_response("test payload invalid"))
            }
        }

        let outcome = extract_chunk(
            Chunk {
                document_id: DocumentId(String::from("stdin-document")),
                text: NonEmptyString(String::from("An apple is a red fruit that grows on trees")),
                token_count: TokenCount(10),
            },
            &EvidenceClient,
        )
        .expect("outcome");

        assert_eq!(outcome.relationships.len(), 1);
        assert_eq!(outcome.relationships[0].evidence.len(), 1);
        assert_eq!(
            outcome.relationships[0].evidence[0].citation_text,
            "apple is a red fruit"
        );
        assert_eq!(
            outcome.relationships[0].evidence[0].citation.document_id.0,
            "stdin-document"
        );
        assert!(matches!(
            outcome.relationships[0].evidence[0].status,
            crate::domain::EpistemicStatus::Probable
        ));
    }

    #[test]
    fn drops_relationship_when_all_evidence_items_are_invalid() {
        struct InvalidEvidenceClient;

        impl SchemaLlmClient for InvalidEvidenceClient {
            fn generate_with_schema<T>(
                &self,
                _sys_prompt: NonEmptyString,
                _user_prompt: NonEmptyString,
            ) -> Result<T, AppError>
            where
                T: serde::de::DeserializeOwned + JsonSchema + 'static,
            {
                let payload = if std::any::TypeId::of::<T>()
                    == std::any::TypeId::of::<AiExtractionResponse>()
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
                            }
                        ]
                    })
                } else {
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
                                        "fact": "Not grounded",
                                        "citation_text": "pear is blue"
                                    }
                                ]
                            }
                        ]
                    })
                };

                serde_json::from_value(payload)
                    .map_err(|_| AppError::provider_response("test payload invalid"))
            }
        }

        let outcome = extract_chunk(
            Chunk {
                document_id: DocumentId(String::from("stdin-document")),
                text: NonEmptyString(String::from("An apple is a red fruit that grows on trees")),
                token_count: TokenCount(10),
            },
            &InvalidEvidenceClient,
        )
        .expect("outcome");

        assert!(outcome.relationships.is_empty());
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
