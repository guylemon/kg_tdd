use super::{EdgeId, NodeId, RelationshipType};

pub(crate) fn edge_id_for_relationship(
    source: &NodeId,
    target: &NodeId,
    relationship_type: &RelationshipType,
) -> EdgeId {
    EdgeId(format!(
        "edge:{}->{}:{}",
        source.0,
        target.0,
        relationship_type_slug(relationship_type)
    ))
}

fn relationship_type_slug(relationship_type: &RelationshipType) -> &'static str {
    match relationship_type {
        RelationshipType::GrowsOn => "grows-on",
        RelationshipType::IsA => "is-a",
    }
}

#[cfg(test)]
mod tests {
    use super::edge_id_for_relationship;
    use crate::domain::{NodeId, RelationshipType};

    #[test]
    fn is_stable_for_repeated_calls() {
        let source = NodeId(String::from("node:product:apple"));
        let target = NodeId(String::from("node:lifeform:tree"));

        let first = edge_id_for_relationship(&source, &target, &RelationshipType::GrowsOn);
        let second = edge_id_for_relationship(&source, &target, &RelationshipType::GrowsOn);

        assert_eq!(first.0, second.0);
        assert_eq!(
            first.0,
            "edge:node:product:apple->node:lifeform:tree:grows-on"
        );
    }

    #[test]
    fn differs_when_relationship_type_differs() {
        let source = NodeId(String::from("node:product:apple"));
        let target = NodeId(String::from("node:lifeform:tree"));

        let grows_on = edge_id_for_relationship(&source, &target, &RelationshipType::GrowsOn);
        let is_a = edge_id_for_relationship(&source, &target, &RelationshipType::IsA);

        assert_ne!(grows_on.0, is_a.0);
    }

    #[test]
    fn differs_when_direction_differs() {
        let apple = NodeId(String::from("node:product:apple"));
        let tree = NodeId(String::from("node:lifeform:tree"));

        let forward = edge_id_for_relationship(&apple, &tree, &RelationshipType::GrowsOn);
        let reverse = edge_id_for_relationship(&tree, &apple, &RelationshipType::GrowsOn);

        assert_ne!(forward.0, reverse.0);
    }
}
