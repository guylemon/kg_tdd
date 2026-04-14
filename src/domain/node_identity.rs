use super::{EntityName, EntityType, NodeId, canonical_entity_name};

pub(crate) fn node_id_for_entity(entity_type: &EntityType, name: &EntityName) -> NodeId {
    let normalized_name = canonical_entity_name(entity_type, name);
    let entity_type_slug = entity_type_slug(entity_type);

    NodeId(format!(
        "node:{}:{}",
        entity_type_slug,
        slugify_normalized_name(&normalized_name)
    ))
}

fn slugify_normalized_name(value: &str) -> String {
    value.replace(' ', "-")
}

fn entity_type_slug(entity_type: &EntityType) -> &'static str {
    match entity_type {
        EntityType::Concept => "concept",
        EntityType::Event => "event",
        EntityType::Lifeform => "lifeform",
        EntityType::Location => "location",
        EntityType::Organization => "organization",
        EntityType::Person => "person",
        EntityType::Product => "product",
        EntityType::Technology => "technology",
    }
}

#[cfg(test)]
mod tests {
    use super::node_id_for_entity;
    use crate::domain::{EntityName, EntityType};

    #[test]
    fn normalizes_case_whitespace_and_punctuation() {
        let id = node_id_for_entity(
            &EntityType::Organization,
            &EntityName(String::from("  ACME,   Inc. ")),
        );

        assert_eq!(id.0, "node:organization:acme-inc");
    }

    #[test]
    fn canonicalizes_conservative_alias_variants() {
        let inc = node_id_for_entity(
            &EntityType::Organization,
            &EntityName(String::from("AT&T Incorporated")),
        );
        let short = node_id_for_entity(
            &EntityType::Organization,
            &EntityName(String::from("AT and T Inc.")),
        );

        assert_eq!(inc.0, short.0);
        assert_eq!(inc.0, "node:organization:at-and-t-inc");
    }

    #[test]
    fn keeps_entity_types_distinct() {
        let person_id = node_id_for_entity(&EntityType::Person, &EntityName(String::from("Apple")));
        let product_id =
            node_id_for_entity(&EntityType::Product, &EntityName(String::from("Apple")));

        assert_ne!(person_id.0, product_id.0);
    }
}
