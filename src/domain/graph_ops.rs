use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::domain::{
    EdgeWeight, EntityMention, EntityName, GraphEdge, GraphNode, NodeId, RelationshipMention,
};

pub(crate) fn consolidate_relationships(
    relationship_mentions: Vec<RelationshipMention>,
) -> Vec<GraphEdge> {
    let mut lookup: HashMap<(NodeId, NodeId), GraphEdge> = HashMap::new();

    for mention in relationship_mentions {
        match lookup.entry((mention.source.clone(), mention.target.clone())) {
            Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().evidence.extend(mention.evidence);
                occupied_entry.get_mut().weight.0 += 1;
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(GraphEdge {
                    source: mention.source,
                    target: mention.target,
                    edge_type: mention.relationship_type,
                    description: mention.description,
                    evidence: mention.evidence,
                    weight: EdgeWeight(1),
                });
            }
        }
    }

    lookup.into_values().collect()
}

pub(crate) fn consolidate_entities(entity_mentions: Vec<EntityMention>) -> Vec<GraphNode> {
    let mut lookup: HashMap<EntityName, GraphNode> = HashMap::new();

    for mention in entity_mentions {
        match lookup.entry(mention.name.clone()) {
            Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().mentions.push(mention.source);
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(GraphNode {
                    description: mention.description,
                    entity_type: mention.entity_type,
                    mentions: vec![mention.source],
                    name: mention.name,
                });
            }
        }
    }

    lookup.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::{consolidate_entities, consolidate_relationships};
    use crate::domain::{
        AnnotatedText, DocumentId, EdgeDescription, EntityMention, EntityName, EntityType, Fact,
        FactualClaim, NodeDescription, NodeId, RelationshipMention, TextUnit, Todo, TokenCount,
    };

    #[test]
    fn merges_entity_mentions_by_name() {
        let mention_a = EntityMention {
            description: NodeDescription(String::from("desc")),
            entity_type: EntityType::Person,
            name: EntityName(String::from("Alice")),
            source: sample_text_unit(),
        };
        let mention_b = EntityMention {
            description: NodeDescription(String::from("desc")),
            entity_type: EntityType::Person,
            name: EntityName(String::from("Alice")),
            source: sample_text_unit(),
        };

        let nodes = consolidate_entities(vec![mention_a, mention_b]);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].mentions.len(), 2);
    }

    #[test]
    fn merges_relationship_mentions_by_endpoints() {
        let relationship_a = RelationshipMention {
            source: NodeId(String::from("alice")),
            target: NodeId(String::from("bob")),
            description: EdgeDescription(String::from("knows")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("Alice knows Bob")),
                citation: sample_text_unit(),
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: Todo,
        };
        let relationship_b = RelationshipMention {
            source: NodeId(String::from("alice")),
            target: NodeId(String::from("bob")),
            description: EdgeDescription(String::from("knows")),
            evidence: Vec::new(),
            relationship_type: Todo,
        };

        let edges = consolidate_relationships(vec![relationship_a, relationship_b]);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].weight.0, 2);
        assert_eq!(edges[0].evidence.len(), 1);
    }

    fn sample_text_unit() -> TextUnit {
        TextUnit {
            document_id: DocumentId(String::from("doc-1")),
            text: AnnotatedText(String::from("Alice met Bob")),
            token_count: TokenCount(4),
        }
    }
}
