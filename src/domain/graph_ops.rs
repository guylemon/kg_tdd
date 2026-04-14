use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;

use crate::domain::{
    EdgeWeight, EntityMention, FactualClaim, GraphEdge, GraphNode, NodeId, RelationshipMention,
    RelationshipType, edge_id_for_relationship, node_id_for_entity,
};

pub(crate) fn consolidate_relationships(
    relationship_mentions: Vec<RelationshipMention>,
) -> Vec<GraphEdge> {
    let mut lookup: HashMap<(NodeId, NodeId, RelationshipType), GraphEdge> = HashMap::new();

    for mention in relationship_mentions {
        match lookup.entry((
            mention.source.clone(),
            mention.target.clone(),
            mention.relationship_type.clone(),
        )) {
            Entry::Occupied(mut occupied_entry) => {
                merge_evidence(&mut occupied_entry.get_mut().evidence, mention.evidence);
                occupied_entry.get_mut().weight.0 += 1;
            }
            Entry::Vacant(vacant_entry) => {
                let mut evidence = Vec::new();
                merge_evidence(&mut evidence, mention.evidence);
                vacant_entry.insert(GraphEdge {
                    id: edge_id_for_relationship(
                        &mention.source,
                        &mention.target,
                        &mention.relationship_type,
                    ),
                    source: mention.source,
                    target: mention.target,
                    edge_type: mention.relationship_type,
                    description: mention.description,
                    evidence,
                    weight: EdgeWeight(1),
                });
            }
        }
    }

    lookup.into_values().collect()
}

fn merge_evidence(existing: &mut Vec<FactualClaim>, incoming: Vec<FactualClaim>) {
    let mut seen: HashSet<(String, String, String, String, usize)> =
        existing.iter().map(FactualClaim::dedupe_key).collect();

    for claim in incoming {
        let key = claim.dedupe_key();
        if seen.insert(key) {
            existing.push(claim);
        }
    }
}

pub(crate) fn consolidate_entities(entity_mentions: Vec<EntityMention>) -> Vec<GraphNode> {
    let mut lookup: HashMap<NodeId, GraphNode> = HashMap::new();

    for mention in entity_mentions {
        let node_id = node_id_for_entity(&mention.entity_type, &mention.name);

        match lookup.entry(node_id.clone()) {
            Entry::Occupied(mut occupied_entry) => {
                let existing = occupied_entry.get_mut();
                if mention.name != existing.name && !existing.aliases.contains(&mention.name) {
                    existing.aliases.push(mention.name.clone());
                }
                existing.mentions.push(mention.source);
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(GraphNode {
                    id: node_id,
                    description: mention.description,
                    entity_type: mention.entity_type,
                    mentions: vec![mention.source],
                    name: mention.name,
                    aliases: Vec::new(),
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
        FactualClaim, NodeDescription, NodeId, RelationshipMention, RelationshipType, TextUnit,
        TokenCount, edge_id_for_relationship,
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
        assert_eq!(nodes[0].id.0, "node:person:alice");
    }

    #[test]
    fn merges_entity_mentions_by_canonical_identity() {
        let mention_a = EntityMention {
            description: NodeDescription(String::from("desc")),
            entity_type: EntityType::Organization,
            name: EntityName(String::from("ACME, Inc.")),
            source: sample_text_unit(),
        };
        let mention_b = EntityMention {
            description: NodeDescription(String::from("desc")),
            entity_type: EntityType::Organization,
            name: EntityName(String::from("acme inc")),
            source: sample_text_unit(),
        };

        let nodes = consolidate_entities(vec![mention_a, mention_b]);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id.0, "node:organization:acme-inc");
        assert_eq!(nodes[0].aliases.len(), 1);
        assert_eq!(nodes[0].aliases[0].0, "acme inc");
    }

    #[test]
    fn merges_entity_mentions_by_conservative_alias_variants() {
        let mention_a = EntityMention {
            description: NodeDescription(String::from("desc")),
            entity_type: EntityType::Organization,
            name: EntityName(String::from("AT&T Incorporated")),
            source: sample_text_unit(),
        };
        let mention_b = EntityMention {
            description: NodeDescription(String::from("desc")),
            entity_type: EntityType::Organization,
            name: EntityName(String::from("AT and T Inc.")),
            source: sample_text_unit(),
        };

        let nodes = consolidate_entities(vec![mention_a, mention_b]);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id.0, "node:organization:at-and-t-inc");
        assert_eq!(nodes[0].name.0, "AT&T Incorporated");
        assert_eq!(nodes[0].aliases.len(), 1);
        assert_eq!(nodes[0].aliases[0].0, "AT and T Inc.");
    }

    #[test]
    fn keeps_entity_mentions_separate_when_types_differ() {
        let person = EntityMention {
            description: NodeDescription(String::from("desc")),
            entity_type: EntityType::Person,
            name: EntityName(String::from("Apple")),
            source: sample_text_unit(),
        };
        let product = EntityMention {
            description: NodeDescription(String::from("desc")),
            entity_type: EntityType::Product,
            name: EntityName(String::from("Apple")),
            source: sample_text_unit(),
        };

        let nodes = consolidate_entities(vec![person, product]);

        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn merges_relationship_mentions_by_endpoints_and_type() {
        let relationship_a = RelationshipMention {
            source: NodeId(String::from("alice")),
            target: NodeId(String::from("bob")),
            description: EdgeDescription(String::from("knows")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("Alice knows Bob")),
                citation_text: String::from("Alice met Bob"),
                citation: sample_text_unit(),
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: RelationshipType::IsA,
        };
        let relationship_b = RelationshipMention {
            source: NodeId(String::from("alice")),
            target: NodeId(String::from("bob")),
            description: EdgeDescription(String::from("knows")),
            evidence: Vec::new(),
            relationship_type: RelationshipType::IsA,
        };

        let edges = consolidate_relationships(vec![relationship_a, relationship_b]);

        assert_eq!(edges.len(), 1);
        assert_eq!(
            edges[0].id.0,
            edge_id_for_relationship(
                &NodeId(String::from("alice")),
                &NodeId(String::from("bob")),
                &RelationshipType::IsA,
            )
            .0
        );
        assert_eq!(edges[0].weight.0, 2);
        assert_eq!(edges[0].evidence.len(), 1);
    }

    #[test]
    fn dedupes_duplicate_evidence_within_merged_relationships() {
        let claim_a = FactualClaim {
            fact: Fact(String::from("An apple is a fruit")),
            citation_text: String::from("apple is a fruit"),
            citation: sample_text_unit(),
            status: crate::domain::EpistemicStatus::Probable,
        };
        let claim_b = FactualClaim {
            fact: Fact(String::from("An apple   is a fruit")),
            citation_text: String::from("apple   is a fruit"),
            citation: sample_text_unit(),
            status: crate::domain::EpistemicStatus::Probable,
        };
        let relationship_a = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("fruit")),
            description: EdgeDescription(String::from("is a")),
            evidence: vec![claim_a],
            relationship_type: RelationshipType::IsA,
        };
        let relationship_b = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("fruit")),
            description: EdgeDescription(String::from("is a")),
            evidence: vec![claim_b],
            relationship_type: RelationshipType::IsA,
        };

        let edges = consolidate_relationships(vec![relationship_a, relationship_b]);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].weight.0, 2);
        assert_eq!(edges[0].evidence.len(), 1);
    }

    #[test]
    fn keeps_evidence_distinct_when_fact_differs() {
        let relationship_a = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("fruit")),
            description: EdgeDescription(String::from("is a")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("An apple is a fruit")),
                citation_text: String::from("apple is a fruit"),
                citation: sample_text_unit(),
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: RelationshipType::IsA,
        };
        let relationship_b = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("fruit")),
            description: EdgeDescription(String::from("is a")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("An apple is edible")),
                citation_text: String::from("apple is a fruit"),
                citation: sample_text_unit(),
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: RelationshipType::IsA,
        };

        let edges = consolidate_relationships(vec![relationship_a, relationship_b]);

        assert_eq!(edges[0].evidence.len(), 2);
    }

    #[test]
    fn keeps_evidence_distinct_when_provenance_differs() {
        let relationship_a = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("fruit")),
            description: EdgeDescription(String::from("is a")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("An apple is a fruit")),
                citation_text: String::from("apple is a fruit"),
                citation: sample_text_unit(),
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: RelationshipType::IsA,
        };
        let relationship_b = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("fruit")),
            description: EdgeDescription(String::from("is a")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("An apple is a fruit")),
                citation_text: String::from("apple is a fruit"),
                citation: TextUnit {
                    document_id: DocumentId(String::from("doc-2")),
                    text: AnnotatedText(String::from("apple is a fruit")),
                    token_count: TokenCount(4),
                },
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: RelationshipType::IsA,
        };

        let edges = consolidate_relationships(vec![relationship_a, relationship_b]);

        assert_eq!(edges[0].evidence.len(), 2);
    }

    #[test]
    fn keeps_relationship_mentions_separate_when_types_differ() {
        let relationship_a = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("tree")),
            description: EdgeDescription(String::from("grows on")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("Apples grow on trees")),
                citation_text: String::from("Apples grow on trees"),
                citation: sample_text_unit(),
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: RelationshipType::GrowsOn,
        };
        let relationship_b = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("tree")),
            description: EdgeDescription(String::from("is associated with")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("Apple is associated with trees")),
                citation_text: String::from("Apple is associated with trees"),
                citation: sample_text_unit(),
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: RelationshipType::IsA,
        };

        let edges = consolidate_relationships(vec![relationship_a, relationship_b]);

        assert_eq!(edges.len(), 2);
        assert!(
            edges
                .iter()
                .any(|edge| edge.edge_type == RelationshipType::GrowsOn)
        );
        assert!(
            edges
                .iter()
                .any(|edge| edge.edge_type == RelationshipType::IsA)
        );
    }

    #[test]
    fn merges_same_type_relationship_mentions_even_when_descriptions_differ() {
        let relationship_a = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("fruit")),
            description: EdgeDescription(String::from("is a")),
            evidence: vec![FactualClaim {
                fact: Fact(String::from("An apple is a fruit")),
                citation_text: String::from("An apple is a fruit"),
                citation: sample_text_unit(),
                status: crate::domain::EpistemicStatus::Probable,
            }],
            relationship_type: RelationshipType::IsA,
        };
        let relationship_b = RelationshipMention {
            source: NodeId(String::from("apple")),
            target: NodeId(String::from("fruit")),
            description: EdgeDescription(String::from("belongs to")),
            evidence: Vec::new(),
            relationship_type: RelationshipType::IsA,
        };

        let edges = consolidate_relationships(vec![relationship_a, relationship_b]);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].weight.0, 2);
        assert_eq!(edges[0].description.0, "is a");
    }

    fn sample_text_unit() -> TextUnit {
        TextUnit {
            document_id: DocumentId(String::from("doc-1")),
            text: AnnotatedText(String::from("Alice met Bob")),
            token_count: TokenCount(4),
        }
    }
}
