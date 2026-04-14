use super::EntityName;
use super::EntityType;
use super::NodeDescription;
use super::NodeId;
use super::TextUnit;

#[derive(Clone)]
pub(crate) struct GraphNode {
    pub(crate) id: NodeId,
    pub(crate) description: NodeDescription,
    pub(crate) entity_type: EntityType,
    pub(crate) mentions: Vec<TextUnit>,
    pub(crate) name: EntityName,
    pub(crate) aliases: Vec<EntityName>,
}
