use super::EntityName;
use super::EntityType;
use super::NodeDescription;
use super::TextUnit;

/// An LLM extracted entity from a source ``TextUnit``
#[derive(Clone)]
pub(crate) struct EntityMention {
    pub(crate) description: NodeDescription,
    pub(crate) entity_type: EntityType,
    pub(crate) name: EntityName,
    /// The source of duplicate ``EntityMentions`` will be folded into the ``GraphNode.mentions``
    /// field.
    pub(crate) source: TextUnit,
}
