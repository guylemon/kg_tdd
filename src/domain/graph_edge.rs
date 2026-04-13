use super::EdgeDescription;
use super::EdgeWeight;
use super::FactualClaim;
use super::NodeId;
use super::Todo;

pub(crate) struct GraphEdge {
    pub(crate) source: NodeId,
    pub(crate) target: NodeId,
    pub(crate) edge_type: Todo,
    pub(crate) description: EdgeDescription,
    pub(crate) evidence: Vec<FactualClaim>,
    pub(crate) weight: EdgeWeight,
}
