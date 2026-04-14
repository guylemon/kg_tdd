use super::{EntityName, EntityType};

pub(crate) fn canonical_entity_name(entity_type: &EntityType, name: &EntityName) -> String {
    let normalized = normalize_surface_form(&name.0);
    let tokens = normalized
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    let canonical_tokens = canonicalize_tokens(entity_type, tokens);
    canonical_tokens.join(" ")
}

fn normalize_surface_form(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_was_separator = true;

    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch == '&' {
            push_token(&mut normalized, &mut previous_was_separator, "and");
        } else if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            previous_was_separator = false;
        } else if !previous_was_separator {
            normalized.push(' ');
            previous_was_separator = true;
        }
    }

    if normalized.ends_with(' ') {
        normalized.pop();
    }

    normalized
}

fn push_token(buffer: &mut String, previous_was_separator: &mut bool, token: &str) {
    if !*previous_was_separator {
        buffer.push(' ');
    }
    buffer.push_str(token);
    buffer.push(' ');
    *previous_was_separator = true;
}

fn canonicalize_tokens(entity_type: &EntityType, mut tokens: Vec<String>) -> Vec<String> {
    if matches!(entity_type, EntityType::Organization) {
        canonicalize_organization_suffix(&mut tokens);
    }

    tokens
}

fn canonicalize_organization_suffix(tokens: &mut Vec<String>) {
    if tokens.len() >= 3
        && tokens[tokens.len() - 3] == "l"
        && tokens[tokens.len() - 2] == "l"
        && tokens[tokens.len() - 1] == "c"
    {
        tokens.truncate(tokens.len() - 3);
        tokens.push(String::from("llc"));
        return;
    }

    if let Some(last) = tokens.last_mut() {
        let canonical = match last.as_str() {
            "incorporated" => Some("inc"),
            "company" => Some("co"),
            "corporation" => Some("corp"),
            _ => None,
        };

        if let Some(canonical) = canonical {
            *last = String::from(canonical);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_entity_name;
    use crate::domain::{EntityName, EntityType};

    #[test]
    fn normalizes_case_whitespace_and_punctuation() {
        let canonical = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("  ACME,   Inc. ")),
        );

        assert_eq!(canonical, "acme inc");
    }

    #[test]
    fn normalizes_ampersands_into_and() {
        let ampersand = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("AT&T Research")),
        );
        let and_form = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("AT and T Research")),
        );

        assert_eq!(ampersand, and_form);
        assert_eq!(ampersand, "at and t research");
    }

    #[test]
    fn canonicalizes_common_organization_suffix_variants() {
        let company = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("Acme Company")),
        );
        let co = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("Acme Co.")),
        );
        let corporation = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("Acme Corporation")),
        );
        let corp = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("Acme Corp")),
        );
        let dotted_llc = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("Acme L.L.C.")),
        );
        let plain_llc = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("Acme LLC")),
        );

        assert_eq!(company, co);
        assert_eq!(company, "acme co");
        assert_eq!(corporation, corp);
        assert_eq!(corporation, "acme corp");
        assert_eq!(dotted_llc, plain_llc);
        assert_eq!(dotted_llc, "acme llc");
    }

    #[test]
    fn leaves_non_organization_names_distinct() {
        let person = canonical_entity_name(
            &EntityType::Person,
            &EntityName(String::from("Jane Company")),
        );
        let organization = canonical_entity_name(
            &EntityType::Organization,
            &EntityName(String::from("Jane Company")),
        );

        assert_eq!(person, "jane company");
        assert_eq!(organization, "jane co");
    }
}
