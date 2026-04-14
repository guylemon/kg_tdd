use super::EdgeDescription;
use super::EdgeId;
use super::EdgeWeight;
use super::FactualClaim;
use super::NodeId;
use super::RelationshipType;

#[derive(Clone)]
pub(crate) struct GraphEdge {
    pub(crate) id: EdgeId,
    pub(crate) source: NodeId,
    pub(crate) target: NodeId,
    pub(crate) edge_type: RelationshipType,
    pub(crate) description: EdgeDescription,
    pub(crate) evidence: Vec<FactualClaim>,
    pub(crate) weight: EdgeWeight,
}
