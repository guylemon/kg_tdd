use super::GraphEdge;
use super::GraphNode;

#[derive(Clone)]
pub(crate) struct KnowledgeGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}
