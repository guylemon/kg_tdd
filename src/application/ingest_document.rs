use crate::application::error::AppError;
use crate::application::{
    ChunkExtractionTrace, ChunkTrace, EntityMentionTrace, EvidenceTrace, ExtractionOutcome,
    IngestionTrace, ProviderResponseTrace, RelationshipMentionTrace, TraceableIngestResult,
};
use crate::domain::{
    Document, GraphEdge, GraphNode, KnowledgeGraph, consolidate_entities, consolidate_relationships,
};
use crate::ports::{ChunkExtractor, DocumentPartitioner};

pub(crate) struct IngestDocumentService<E> {
    extractor: E,
}

impl<E> IngestDocumentService<E>
where
    E: ChunkExtractor + DocumentPartitioner,
{
    pub(crate) fn new(extractor: E) -> Self {
        Self { extractor }
    }

    pub(crate) fn execute_with_trace(
        &self,
        document: &Document,
    ) -> Result<TraceableIngestResult, AppError> {
        let chunks = self.extractor.partition(document)?;

        let mut chunk_traces = Vec::with_capacity(chunks.len());
        let mut entity_mentions = Vec::new();
        let mut relationship_mentions = Vec::new();
        let mut provider_responses = Vec::new();
        let mut extracted_mentions = Vec::with_capacity(chunks.len());

        for (index, chunk) in chunks.into_iter().enumerate() {
            chunk_traces.push(ChunkTrace {
                index,
                document_id: chunk.document_id.0.clone(),
                text: chunk.text.0.clone(),
                token_count: chunk.token_count.0,
            });

            let extraction = self.extractor.extract(chunk)?;
            let ExtractionOutcome {
                entities,
                relationships,
                provider_responses: captured_provider_responses,
            } = extraction;
            let entity_traces = entities
                .iter()
                .map(|entity| EntityMentionTrace {
                    name: entity.name.0.clone(),
                    entity_type: format!("{:?}", entity.entity_type),
                    description: entity.description.0.clone(),
                    source_document_id: entity.source.document_id.0.clone(),
                    source_text: entity.source.text.0.clone(),
                    source_token_count: entity.source.token_count.0,
                })
                .collect::<Vec<_>>();
            let relationship_traces = relationships
                .iter()
                .map(|relationship| RelationshipMentionTrace {
                    source: relationship.source.0.clone(),
                    target: relationship.target.0.clone(),
                    relationship_type: format!("{:?}", relationship.relationship_type),
                    description: relationship.description.0.clone(),
                    evidence: relationship
                        .evidence
                        .iter()
                        .map(|claim| EvidenceTrace {
                            fact: claim.fact.0.clone(),
                            citation_text: claim.citation_text.clone(),
                            status: format!("{:?}", claim.status),
                            source_document_id: claim.citation.document_id.0.clone(),
                            source_text: claim.citation.text.0.clone(),
                            source_token_count: claim.citation.token_count.0,
                        })
                        .collect::<Vec<_>>(),
                })
                .collect::<Vec<_>>();
            extracted_mentions.push(ChunkExtractionTrace {
                chunk_index: index,
                entities: entity_traces,
                relationships: relationship_traces,
            });
            provider_responses.extend(captured_provider_responses.into_iter().map(|response| {
                ProviderResponseTrace {
                    chunk_index: index,
                    kind: response.kind,
                    raw_response: response.raw_response,
                }
            }));
            entity_mentions.extend(entities);
            relationship_mentions.extend(relationships);
        }

        let (nodes, edges): (Vec<GraphNode>, Vec<GraphEdge>) =
            finalize_entities_relationships(entity_mentions, relationship_mentions);

        Ok(TraceableIngestResult {
            graph: KnowledgeGraph { nodes, edges },
            trace: IngestionTrace {
                chunks: chunk_traces,
                provider_responses,
                extracted_mentions,
            },
        })
    }
}

fn finalize_entities_relationships(
    entities: Vec<crate::domain::EntityMention>,
    relationships: Vec<crate::domain::RelationshipMention>,
) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let nodes = consolidate_entities(entities);
    let edges = consolidate_relationships(relationships);

    (nodes, edges)
}

#[cfg(test)]
mod tests {
    use super::IngestDocumentService;
    use crate::application::{
        AppError, CapturedProviderResponse, Chunk, ExtractionOutcome, ProviderResponseKind,
    };
    use crate::domain::{
        AnnotatedText, Document, DocumentId, EdgeDescription, EntityMention, EntityName,
        EntityType, Fact, FactualClaim, KnowledgeGraph, NodeDescription, NonEmptyString,
        RelationshipMention, RelationshipType, TextUnit, TokenCount, edge_id_for_relationship,
        node_id_for_entity,
    };
    use crate::ports::{ChunkExtractor, DocumentPartitioner};
    use std::cell::RefCell;

    struct FakeExtractor {
        chunks: Vec<Chunk>,
        extractions: RefCell<Vec<Result<ExtractionOutcome, AppError>>>,
    }

    impl DocumentPartitioner for FakeExtractor {
        fn partition(&self, _document: &Document) -> Result<Vec<Chunk>, AppError> {
            Ok(self.chunks.clone())
        }
    }

    impl ChunkExtractor for FakeExtractor {
        fn extract(&self, _chunk: Chunk) -> Result<ExtractionOutcome, AppError> {
            self.extractions.borrow_mut().remove(0)
        }
    }

    #[test]
    fn returns_empty_graph_when_no_chunks_are_emitted() {
        let extractor = FakeExtractor {
            chunks: Vec::new(),
            extractions: RefCell::new(Vec::new()),
        };
        let service = IngestDocumentService::new(extractor);

        let traced = service
            .execute_with_trace(&Document {
                id: DocumentId(String::new()),
                text: NonEmptyString(String::from("ignored")),
            })
            .expect("empty graph");
        let graph = traced.graph;

        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
        assert!(traced.trace.chunks.is_empty());
        assert!(traced.trace.extracted_mentions.is_empty());
    }

    #[test]
    fn propagates_chunk_extraction_errors() {
        let extractor = FakeExtractor {
            chunks: vec![sample_chunk()],
            extractions: RefCell::new(vec![Err(AppError::ExtractChunk)]),
        };
        let service = IngestDocumentService::new(extractor);

        let result = service.execute_with_trace(&Document {
            id: DocumentId(String::new()),
            text: NonEmptyString(String::from("ignored")),
        });

        assert!(matches!(result, Err(AppError::ExtractChunk)));
    }

    #[test]
    fn consolidates_mentions_into_graph() {
        let chunk = sample_chunk();
        let extractor = FakeExtractor {
            chunks: vec![chunk],
            extractions: RefCell::new(vec![Ok(person_extraction_with_provider_responses())]),
        };
        let service = IngestDocumentService::new(extractor);

        let traced = service
            .execute_with_trace(&Document {
                id: DocumentId(String::from("doc-1")),
                text: NonEmptyString(String::from("Alice met Bob")),
            })
            .expect("trace");

        let graph = traced.graph;

        assert_eq!(traced.trace.chunks.len(), 1);
        assert_eq!(traced.trace.chunks[0].index, 0);
        assert_eq!(traced.trace.chunks[0].document_id, "doc-1");
        assert_eq!(traced.trace.chunks[0].text, "Alice met Bob");
        assert_eq!(traced.trace.chunks[0].token_count, 4);
        assert_eq!(traced.trace.provider_responses.len(), 2);
        assert_eq!(traced.trace.provider_responses[0].chunk_index, 0);
        assert_eq!(
            traced.trace.provider_responses[0].kind,
            ProviderResponseKind::EntityExtraction
        );
        assert!(
            traced.trace.provider_responses[0]
                .raw_response
                .contains("\"entities\"")
        );
        assert_eq!(traced.trace.provider_responses[1].chunk_index, 0);
        assert_eq!(
            traced.trace.provider_responses[1].kind,
            ProviderResponseKind::RelationshipExtraction
        );
        assert_eq!(traced.trace.extracted_mentions.len(), 1);
        assert_eq!(traced.trace.extracted_mentions[0].chunk_index, 0);
        assert_eq!(traced.trace.extracted_mentions[0].entities.len(), 2);
        assert_eq!(traced.trace.extracted_mentions[0].relationships.len(), 1);
        assert_eq!(
            traced.trace.extracted_mentions[0].entities[0].source_document_id,
            "doc-1"
        );
        assert_eq!(
            traced.trace.extracted_mentions[0].relationships[0].evidence[0].source_text,
            "<entity>Alice</entity> met <entity>Bob</entity>"
        );

        assert_single_person_graph(&graph);
    }

    #[test]
    fn consolidates_relationship_endpoints_across_canonical_alias_variants() {
        let chunks = vec![sample_chunk(), sample_chunk()];
        let text_unit = alias_variants_text_unit();
        let extractor = FakeExtractor {
            chunks,
            extractions: RefCell::new(vec![
                Ok(alias_variant_extraction(
                    "AT&T Incorporated",
                    "Acme Company",
                    text_unit.clone(),
                )),
                Ok(alias_variant_extraction(
                    "AT and T Inc.",
                    "Acme Co.",
                    text_unit.clone(),
                )),
            ]),
        };
        let service = IngestDocumentService::new(extractor);

        let traced = service
            .execute_with_trace(&Document {
                id: DocumentId(String::from("doc-1")),
                text: NonEmptyString(String::from("AT&T partnered with Acme")),
            })
            .expect("graph");
        let graph = traced.graph;

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(
            graph.edges[0].id.0,
            edge_id_for_relationship(
                &node_id_for_entity(
                    &EntityType::Organization,
                    &EntityName(String::from("AT&T Incorporated")),
                ),
                &node_id_for_entity(
                    &EntityType::Organization,
                    &EntityName(String::from("Acme Company")),
                ),
                &RelationshipType::IsA,
            )
            .0
        );
        assert_eq!(graph.edges[0].weight.0, 2);
        assert_eq!(graph.edges[0].source.0, "node:organization:at-and-t-inc");
        assert_eq!(graph.edges[0].target.0, "node:organization:acme-co");
    }

    #[test]
    fn keeps_distinct_edges_for_same_endpoints_when_relationship_types_differ() {
        let chunk = sample_chunk();
        let text_unit = TextUnit {
            document_id: DocumentId(String::from("doc-1")),
            text: AnnotatedText(String::from("Apple and trees are related in two ways")),
            token_count: TokenCount(9),
        };
        let apple_id = node_id_for_entity(&EntityType::Product, &EntityName(String::from("apple")));
        let tree_id = node_id_for_entity(&EntityType::Lifeform, &EntityName(String::from("trees")));
        let extractor = FakeExtractor {
            chunks: vec![chunk],
            extractions: RefCell::new(vec![Ok(ExtractionOutcome {
                entities: vec![
                    EntityMention {
                        description: NodeDescription(String::from("product")),
                        entity_type: EntityType::Product,
                        name: EntityName(String::from("apple")),
                        source: text_unit.clone(),
                    },
                    EntityMention {
                        description: NodeDescription(String::from("lifeform")),
                        entity_type: EntityType::Lifeform,
                        name: EntityName(String::from("trees")),
                        source: text_unit.clone(),
                    },
                ],
                relationships: vec![
                    RelationshipMention {
                        source: apple_id.clone(),
                        target: tree_id.clone(),
                        description: EdgeDescription(String::from("is associated with")),
                        evidence: vec![FactualClaim {
                            fact: Fact(String::from("Apple is associated with trees")),
                            citation_text: String::from("Apple and trees are related"),
                            citation: text_unit.clone(),
                            status: crate::domain::EpistemicStatus::Probable,
                        }],
                        relationship_type: RelationshipType::IsA,
                    },
                    RelationshipMention {
                        source: apple_id,
                        target: tree_id,
                        description: EdgeDescription(String::from("grows on")),
                        evidence: vec![FactualClaim {
                            fact: Fact(String::from("An apple grows on trees")),
                            citation_text: String::from("Apple and trees are related"),
                            citation: text_unit.clone(),
                            status: crate::domain::EpistemicStatus::Probable,
                        }],
                        relationship_type: RelationshipType::GrowsOn,
                    },
                ],
                provider_responses: Vec::new(),
            })]),
        };
        let service = IngestDocumentService::new(extractor);

        let traced = service
            .execute_with_trace(&Document {
                id: DocumentId(String::from("doc-1")),
                text: NonEmptyString(String::from("Apple and trees are related in two ways")),
            })
            .expect("graph");
        let graph = traced.graph;

        assert_eq!(graph.edges.len(), 2);
        assert!(
            graph
                .edges
                .iter()
                .any(|edge| edge.edge_type == RelationshipType::IsA)
        );
        assert!(graph.edges.iter().any(|edge| {
            edge.id.0
                == edge_id_for_relationship(
                    &node_id_for_entity(&EntityType::Product, &EntityName(String::from("apple"))),
                    &node_id_for_entity(&EntityType::Lifeform, &EntityName(String::from("trees"))),
                    &RelationshipType::IsA,
                )
                .0
        }));
        assert!(
            graph
                .edges
                .iter()
                .any(|edge| edge.edge_type == RelationshipType::GrowsOn)
        );
        assert!(graph.edges.iter().any(|edge| {
            edge.id.0
                == edge_id_for_relationship(
                    &node_id_for_entity(&EntityType::Product, &EntityName(String::from("apple"))),
                    &node_id_for_entity(&EntityType::Lifeform, &EntityName(String::from("trees"))),
                    &RelationshipType::GrowsOn,
                )
                .0
        }));
    }

    fn sample_chunk() -> Chunk {
        Chunk {
            document_id: DocumentId(String::from("doc-1")),
            text: NonEmptyString(String::from("Alice met Bob")),
            token_count: TokenCount(4),
        }
    }

    fn alias_variants_text_unit() -> TextUnit {
        TextUnit {
            document_id: DocumentId(String::from("doc-1")),
            text: AnnotatedText(String::from("AT&T partnered with Acme")),
            token_count: TokenCount(5),
        }
    }

    fn alias_variant_extraction(
        source_name: &str,
        target_name: &str,
        text_unit: TextUnit,
    ) -> ExtractionOutcome {
        ExtractionOutcome {
            entities: vec![
                organization_mention(source_name, text_unit.clone()),
                organization_mention(target_name, text_unit.clone()),
            ],
            relationships: vec![organization_relationship_mention(
                source_name,
                target_name,
                text_unit,
            )],
            provider_responses: Vec::new(),
        }
    }

    fn person_extraction_with_provider_responses() -> ExtractionOutcome {
        let mut extraction = person_extraction_without_provider_responses();
        extraction.provider_responses = vec![
            CapturedProviderResponse {
                kind: ProviderResponseKind::EntityExtraction,
                raw_response: String::from("{\"entities\":[{\"name\":\"Alice\"}]}"),
            },
            CapturedProviderResponse {
                kind: ProviderResponseKind::RelationshipExtraction,
                raw_response: String::from("{\"relationships\":[{\"description\":\"knows\"}]}"),
            },
        ];
        extraction
    }

    fn person_extraction_without_provider_responses() -> ExtractionOutcome {
        let text_unit = TextUnit {
            document_id: DocumentId(String::from("doc-1")),
            text: AnnotatedText(String::from(
                "<entity>Alice</entity> met <entity>Bob</entity>",
            )),
            token_count: TokenCount(8),
        };

        ExtractionOutcome {
            entities: vec![
                EntityMention {
                    description: NodeDescription(String::from("person")),
                    entity_type: EntityType::Person,
                    name: EntityName(String::from("Alice")),
                    source: text_unit.clone(),
                },
                EntityMention {
                    description: NodeDescription(String::from("person")),
                    entity_type: EntityType::Person,
                    name: EntityName(String::from("Alice")),
                    source: text_unit.clone(),
                },
            ],
            relationships: vec![RelationshipMention {
                source: node_id_for_entity(&EntityType::Person, &EntityName(String::from("Alice"))),
                target: node_id_for_entity(&EntityType::Person, &EntityName(String::from("Bob"))),
                description: EdgeDescription(String::from("knows")),
                evidence: vec![FactualClaim {
                    fact: Fact(String::from("Alice met Bob")),
                    citation_text: String::from("Alice met Bob"),
                    citation: text_unit,
                    status: crate::domain::EpistemicStatus::Probable,
                }],
                relationship_type: RelationshipType::IsA,
            }],
            provider_responses: Vec::new(),
        }
    }

    fn assert_single_person_graph(graph: &KnowledgeGraph) {
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].mentions.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(
            graph.edges[0].id.0,
            edge_id_for_relationship(
                &node_id_for_entity(&EntityType::Person, &EntityName(String::from("Alice"))),
                &node_id_for_entity(&EntityType::Person, &EntityName(String::from("Bob"))),
                &RelationshipType::IsA,
            )
            .0
        );
        assert_eq!(graph.edges[0].evidence.len(), 1);
        assert_eq!(graph.edges[0].evidence[0].citation_text, "Alice met Bob");
    }

    fn organization_mention(name: &str, source: TextUnit) -> EntityMention {
        EntityMention {
            description: NodeDescription(String::from("org")),
            entity_type: EntityType::Organization,
            name: EntityName(String::from(name)),
            source,
        }
    }

    fn organization_relationship_mention(
        source_name: &str,
        target_name: &str,
        citation: TextUnit,
    ) -> RelationshipMention {
        RelationshipMention {
            source: node_id_for_entity(
                &EntityType::Organization,
                &EntityName(String::from(source_name)),
            ),
            target: node_id_for_entity(
                &EntityType::Organization,
                &EntityName(String::from(target_name)),
            ),
            description: EdgeDescription(String::from("partnered with")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("AT&T partnered with Acme")),
                citation_text: String::from("AT&T partnered with Acme"),
                citation,
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: RelationshipType::IsA,
        }
    }
}
