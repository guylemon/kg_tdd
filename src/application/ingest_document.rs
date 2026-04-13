use crate::application::error::AppError;
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

    pub(crate) fn execute(&self, document: &Document) -> Result<KnowledgeGraph, AppError> {
        let chunks = self.extractor.partition(document)?;

        let mut entity_mentions = Vec::new();
        let mut relationship_mentions = Vec::new();

        for chunk in chunks {
            let extraction = self.extractor.extract(chunk)?;
            entity_mentions.extend(extraction.entities);
            relationship_mentions.extend(extraction.relationships);
        }

        let (nodes, edges): (Vec<GraphNode>, Vec<GraphEdge>) =
            finalize_entities_relationships(entity_mentions, relationship_mentions);

        Ok(KnowledgeGraph { nodes, edges })
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
    use crate::application::{AppError, Chunk, ExtractionOutcome};
    use crate::domain::{
        AnnotatedText, Document, DocumentId, EdgeDescription, EntityMention, EntityName,
        EntityType, Fact, FactualClaim, NodeDescription, NonEmptyString, RelationshipMention,
        RelationshipType, TextUnit, TokenCount,
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

        let graph = service
            .execute(&Document {
                id: DocumentId(String::new()),
                text: NonEmptyString(String::from("ignored")),
            })
            .expect("empty graph");

        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn propagates_chunk_extraction_errors() {
        let extractor = FakeExtractor {
            chunks: vec![sample_chunk()],
            extractions: RefCell::new(vec![Err(AppError::ExtractChunk)]),
        };
        let service = IngestDocumentService::new(extractor);

        let result = service.execute(&Document {
            id: DocumentId(String::new()),
            text: NonEmptyString(String::from("ignored")),
        });

        assert!(matches!(result, Err(AppError::ExtractChunk)));
    }

    #[test]
    fn consolidates_mentions_into_graph() {
        let chunk = sample_chunk();
        let text_unit = TextUnit {
            document_id: DocumentId(String::from("doc-1")),
            text: AnnotatedText(String::from(
                "<entity>Alice</entity> met <entity>Bob</entity>",
            )),
            token_count: TokenCount(8),
        };
        let extractor = FakeExtractor {
            chunks: vec![chunk],
            extractions: RefCell::new(vec![Ok(ExtractionOutcome {
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
                    source: crate::domain::NodeId(String::from("Alice")),
                    target: crate::domain::NodeId(String::from("Bob")),
                    description: EdgeDescription(String::from("knows")),
                    evidence: vec![FactualClaim {
                        fact: Fact(String::from("Alice met Bob")),
                        citation_text: String::from("Alice met Bob"),
                        citation: text_unit.clone(),
                        status: crate::domain::EpistemicStatus::Probable,
                    }],
                    relationship_type: RelationshipType::IsA,
                }],
            })]),
        };
        let service = IngestDocumentService::new(extractor);

        let graph = service
            .execute(&Document {
                id: DocumentId(String::from("doc-1")),
                text: NonEmptyString(String::from("Alice met Bob")),
            })
            .expect("graph");

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].mentions.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].evidence.len(), 1);
        assert_eq!(graph.edges[0].evidence[0].citation_text, "Alice met Bob");
    }

    fn sample_chunk() -> Chunk {
        Chunk {
            document_id: DocumentId(String::from("doc-1")),
            text: NonEmptyString(String::from("Alice met Bob")),
            token_count: TokenCount(4),
        }
    }
}
