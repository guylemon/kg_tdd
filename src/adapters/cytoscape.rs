use crate::application::AppError;
use crate::domain::{
    EdgeDescription, EdgeId, EdgeWeight, EntityName, EntityType, FactualClaim, GraphEdge,
    GraphNode, KnowledgeGraph, NodeDescription, NodeId, TextUnit, Todo,
};
use serde::Serialize;

const TODO_STRING: &str = "";

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
    entity_type: EntityType,
    description: NodeDescription,
    mentions: Vec<TextUnit>,
}

impl From<GraphNode> for CyNodeData {
    fn from(node: GraphNode) -> Self {
        Self {
            id: NodeId(TODO_STRING.to_owned()),
            name: node.name,
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
    edge_type: Todo,
    description: EdgeDescription,
    evidence: Vec<FactualClaim>,
    weight: EdgeWeight,
}

impl From<GraphEdge> for CyEdgeData {
    fn from(edge: GraphEdge) -> Self {
        Self {
            id: EdgeId(TODO_STRING.to_owned()),
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
