use super::EntityName;
use super::EntityType;
use super::NodeDescription;
use super::TextUnit;

pub(crate) struct GraphNode {
    pub(crate) description: NodeDescription,
    pub(crate) entity_type: EntityType,
    pub(crate) mentions: Vec<TextUnit>,
    pub(crate) name: EntityName,
}
