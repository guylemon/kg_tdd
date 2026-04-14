use crate::application::AppError;
use crate::domain::{
    EdgeDescription, EdgeId, EdgeWeight, EntityName, EntityType, FactualClaim, GraphEdge,
    GraphNode, KnowledgeGraph, NodeDescription, NodeId, RelationshipType, TextUnit,
};
use serde::Serialize;

/// A serializable type used as input to the cytoscape JavaScript UI library.
/// See more: <https://js.cytoscape.org/#notation/elements-json>
#[derive(Debug, Serialize)]
struct CytoscapeElements {
    nodes: Vec<CyNode>,
    edges: Vec<CyEdge>,
}

#[derive(Debug, Serialize)]
struct CyNode {
    data: CyNodeData,
}

#[derive(Debug, Serialize)]
struct CyEdge {
    data: CyEdgeData,
}

#[derive(Debug, Serialize)]
struct CyNodeData {
    id: NodeId,
    name: EntityName,
    aliases: Vec<EntityName>,
    entity_type: EntityType,
    description: NodeDescription,
    mentions: Vec<TextUnit>,
}

impl From<GraphNode> for CyNodeData {
    fn from(node: GraphNode) -> Self {
        Self {
            id: node.id,
            name: node.name,
            aliases: node.aliases,
            entity_type: node.entity_type,
            description: node.description,
            mentions: node.mentions,
        }
    }
}

#[derive(Debug, Serialize)]
struct CyEdgeData {
    id: EdgeId,
    source: NodeId,
    target: NodeId,
    edge_type: RelationshipType,
    description: EdgeDescription,
    evidence: Vec<FactualClaim>,
    weight: EdgeWeight,
}

impl From<GraphEdge> for CyEdgeData {
    fn from(edge: GraphEdge) -> Self {
        Self {
            id: edge.id,
            source: edge.source,
            target: edge.target,
            edge_type: edge.edge_type,
            description: edge.description,
            evidence: edge.evidence,
            weight: edge.weight,
        }
    }
}

pub(crate) struct CytoscapeJsonProjector;

impl CytoscapeJsonProjector {
    pub(crate) fn project(kg: &KnowledgeGraph) -> Result<String, AppError> {
        let elements = convert_kg_to_cytoscape_elements(kg);
        serde_json::to_string(&elements).map_err(|_| AppError::ProjectGraph)
    }
}

fn convert_kg_to_cytoscape_elements(kg: &KnowledgeGraph) -> CytoscapeElements {
    let nodes = kg
        .nodes
        .iter()
        .cloned()
        .map(|n| CyNode { data: n.into() })
        .collect();

    let edges = kg
        .edges
        .iter()
        .cloned()
        .map(|e| CyEdge { data: e.into() })
        .collect();

    CytoscapeElements { nodes, edges }
}

#[cfg(test)]
mod tests {
    use super::CytoscapeJsonProjector;
    use crate::domain::{
        EdgeDescription, EdgeWeight, EntityName, EntityType, GraphEdge, GraphNode, KnowledgeGraph,
        NodeDescription, NodeId, RelationshipType, edge_id_for_relationship,
    };

    #[test]
    fn projects_distinct_edge_ids_for_same_endpoints_with_different_types() {
        let graph = KnowledgeGraph {
            nodes: vec![
                GraphNode {
                    id: NodeId(String::from("node:food:apple")),
                    name: EntityName(String::from("apple")),
                    aliases: Vec::new(),
                    entity_type: EntityType::Product,
                    description: NodeDescription(String::from("food")),
                    mentions: Vec::new(),
                },
                GraphNode {
                    id: NodeId(String::from("node:plant:tree")),
                    name: EntityName(String::from("tree")),
                    aliases: Vec::new(),
                    entity_type: EntityType::Lifeform,
                    description: NodeDescription(String::from("plant")),
                    mentions: Vec::new(),
                },
            ],
            edges: vec![
                GraphEdge {
                    id: edge_id_for_relationship(
                        &NodeId(String::from("node:food:apple")),
                        &NodeId(String::from("node:plant:tree")),
                        &RelationshipType::GrowsOn,
                    ),
                    source: NodeId(String::from("node:food:apple")),
                    target: NodeId(String::from("node:plant:tree")),
                    edge_type: RelationshipType::GrowsOn,
                    description: EdgeDescription(String::from("grows on")),
                    evidence: Vec::new(),
                    weight: EdgeWeight(1),
                },
                GraphEdge {
                    id: edge_id_for_relationship(
                        &NodeId(String::from("node:food:apple")),
                        &NodeId(String::from("node:plant:tree")),
                        &RelationshipType::IsA,
                    ),
                    source: NodeId(String::from("node:food:apple")),
                    target: NodeId(String::from("node:plant:tree")),
                    edge_type: RelationshipType::IsA,
                    description: EdgeDescription(String::from("is related to")),
                    evidence: Vec::new(),
                    weight: EdgeWeight(1),
                },
            ],
        };

        let projected = CytoscapeJsonProjector::project(&graph).expect("projection succeeds");

        assert!(projected.contains("\"id\":\"edge:node:food:apple->node:plant:tree:grows-on\""));
        assert!(projected.contains("\"id\":\"edge:node:food:apple->node:plant:tree:is-a\""));
    }
}
