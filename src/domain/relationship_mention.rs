use super::EdgeDescription;
use super::FactualClaim;
use super::NodeId;
use super::Todo;

pub(crate) struct RelationshipMention {
    pub(crate) source: NodeId,
    pub(crate) target: NodeId,
    pub(crate) description: EdgeDescription,
    pub(crate) evidence: Vec<FactualClaim>,
    pub(crate) relationship_type: Todo,
}
