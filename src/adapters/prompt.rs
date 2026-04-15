use std::fs;
use std::path::Path;

use crate::application::AppError;
use crate::domain::{NonEmptyString, TextUnit};

pub(crate) struct PromptPair {
    pub(crate) system: NonEmptyString,
    pub(crate) user: NonEmptyString,
}

#[derive(Clone, Debug)]
pub(crate) struct PromptTemplates {
    entity_system: ParsedTemplate,
    entity_user: ParsedTemplate,
    relationship_system: ParsedTemplate,
    relationship_user: ParsedTemplate,
}

impl PromptTemplates {
    pub(crate) fn load(root: &Path) -> Result<Self, AppError> {
        Ok(Self {
            entity_system: load_template(
                root,
                ENTITY_SYSTEM_TEMPLATE_FILE,
                "entity system prompt",
                &[],
            )?,
            entity_user: load_template(
                root,
                ENTITY_USER_TEMPLATE_FILE,
                "entity user prompt",
                &["input_text"],
            )?,
            relationship_system: load_template(
                root,
                RELATIONSHIP_SYSTEM_TEMPLATE_FILE,
                "relationship system prompt",
                &[],
            )?,
            relationship_user: load_template(
                root,
                RELATIONSHIP_USER_TEMPLATE_FILE,
                "relationship user prompt",
                &["annotated_text"],
            )?,
        })
    }
    pub(crate) fn build_entity_prompt(
        &self,
        text: &NonEmptyString,
    ) -> Result<PromptPair, AppError> {
        Ok(PromptPair {
            system: self.entity_system.render("entity system prompt", &[])?,
            user: self
                .entity_user
                .render("entity user prompt", &[("input_text", &text.0)])?,
        })
    }

    pub(crate) fn build_relationship_prompt(
        &self,
        text_unit: &TextUnit,
    ) -> Result<PromptPair, AppError> {
        Ok(PromptPair {
            system: self
                .relationship_system
                .render("relationship system prompt", &[])?,
            user: self.relationship_user.render(
                "relationship user prompt",
                &[("annotated_text", &text_unit.text.0)],
            )?,
        })
    }
}

pub(crate) struct EntityExtractionPrompt<'a> {
    templates: &'a PromptTemplates,
    text: &'a NonEmptyString,
}

impl<'a> EntityExtractionPrompt<'a> {
    pub(crate) fn new(templates: &'a PromptTemplates, text: &'a NonEmptyString) -> Self {
        Self { templates, text }
    }

    pub(crate) fn build(&self) -> Result<PromptPair, AppError> {
        self.templates.build_entity_prompt(self.text)
    }
}

pub(crate) struct RelationshipExtractionPrompt<'a> {
    templates: &'a PromptTemplates,
    text_unit: &'a TextUnit,
}

impl<'a> RelationshipExtractionPrompt<'a> {
    pub(crate) fn new(templates: &'a PromptTemplates, text_unit: &'a TextUnit) -> Self {
        Self {
            templates,
            text_unit,
        }
    }

    pub(crate) fn build(&self) -> Result<PromptPair, AppError> {
        self.templates.build_relationship_prompt(self.text_unit)
    }
}

#[derive(Clone, Debug)]
struct ParsedTemplate {
    segments: Vec<TemplateSegment>,
}

#[derive(Clone, Debug)]
enum TemplateSegment {
    Literal(String),
    Variable(String),
}

impl ParsedTemplate {
    fn parse(raw: &str, label: &str, allowed_variables: &[&str]) -> Result<Self, AppError> {
        if raw.is_empty() {
            return Err(AppError::invalid_prompt_template(format!(
                "{label} is empty"
            )));
        }

        let mut segments = Vec::new();
        let mut remainder = raw;

        while let Some(start) = remainder.find("{{") {
            let (literal, after_literal) = remainder.split_at(start);
            if !literal.is_empty() {
                segments.push(TemplateSegment::Literal(literal.to_owned()));
            }

            let after_open = &after_literal[2..];
            let end = after_open.find("}}").ok_or_else(|| {
                AppError::invalid_prompt_template(format!("{label} has an unclosed placeholder"))
            })?;

            let placeholder = after_open[..end].trim();
            if placeholder.is_empty() {
                return Err(AppError::invalid_prompt_template(format!(
                    "{label} has an empty placeholder"
                )));
            }
            if !is_valid_placeholder_name(placeholder) {
                return Err(AppError::invalid_prompt_template(format!(
                    "{label} has an invalid placeholder name: {placeholder}"
                )));
            }
            if !allowed_variables.contains(&placeholder) {
                return Err(AppError::invalid_prompt_template(format!(
                    "{label} uses unknown placeholder: {placeholder}"
                )));
            }

            segments.push(TemplateSegment::Variable(placeholder.to_owned()));
            remainder = &after_open[end + 2..];
        }

        if !remainder.is_empty() {
            segments.push(TemplateSegment::Literal(remainder.to_owned()));
        }

        Ok(Self { segments })
    }

    fn render(
        &self,
        label: &str,
        substitutions: &[(&str, &str)],
    ) -> Result<NonEmptyString, AppError> {
        let mut rendered = String::new();

        for segment in &self.segments {
            match segment {
                TemplateSegment::Literal(text) => rendered.push_str(text),
                TemplateSegment::Variable(name) => {
                    let value = substitutions
                        .iter()
                        .find(|(key, _)| *key == name.as_str())
                        .map(|(_, value)| *value)
                        .ok_or_else(|| {
                            AppError::invalid_prompt_template(format!(
                                "{label} is missing a value for placeholder: {name}"
                            ))
                        })?;
                    rendered.push_str(value);
                }
            }
        }

        if rendered.is_empty() {
            return Err(AppError::invalid_prompt_template(format!(
                "{label} rendered to an empty prompt"
            )));
        }

        Ok(NonEmptyString(rendered))
    }
}

const ENTITY_SYSTEM_TEMPLATE_FILE: &str = "entity.system.txt";
const ENTITY_USER_TEMPLATE_FILE: &str = "entity.user.txt";
const RELATIONSHIP_SYSTEM_TEMPLATE_FILE: &str = "relationship.system.txt";
const RELATIONSHIP_USER_TEMPLATE_FILE: &str = "relationship.user.txt";

fn load_template(
    root: &Path,
    file_name: &str,
    label: &str,
    allowed_variables: &[&str],
) -> Result<ParsedTemplate, AppError> {
    let path = root.join(file_name);
    let raw = fs::read_to_string(&path).map_err(|_| AppError::read_prompt_template(path))?;
    ParsedTemplate::parse(&raw, label, allowed_variables)
}

fn is_valid_placeholder_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::domain::{AnnotatedText, DocumentId, NonEmptyString, TextUnit, TokenCount};

    use super::{ParsedTemplate, PromptTemplates};

    #[test]
    fn loads_and_renders_templates_from_directory() {
        let root = temp_dir("prompt-templates");
        write_prompt_templates(
            &root,
            "Entity system.",
            "Entity input: {{input_text}}",
            "Relationship system.",
            "Relationship input: {{annotated_text}}",
        );

        let templates = PromptTemplates::load(&root).expect("prompt templates");
        let entity_prompt = templates
            .build_entity_prompt(&NonEmptyString(String::from("An apple is a fruit.")))
            .expect("entity prompt");
        let relationship_prompt = templates
            .build_relationship_prompt(&TextUnit {
                document_id: DocumentId(String::from("doc-1")),
                text: AnnotatedText(String::from(
                    "<entity type=\"Concept\">apple</entity> is a fruit.",
                )),
                token_count: TokenCount(7),
            })
            .expect("relationship prompt");

        assert_eq!(entity_prompt.system.0, "Entity system.");
        assert_eq!(entity_prompt.user.0, "Entity input: An apple is a fruit.");
        assert_eq!(relationship_prompt.system.0, "Relationship system.");
        assert_eq!(
            relationship_prompt.user.0,
            "Relationship input: <entity type=\"Concept\">apple</entity> is a fruit."
        );
    }

    #[test]
    fn rejects_unknown_placeholder() {
        let root = temp_dir("prompt-template-unknown-placeholder");
        write_prompt_templates(
            &root,
            "Entity system.",
            "Entity input: {{input_text}}",
            "Relationship system.",
            "Relationship input: {{unknown}}",
        );

        let err = PromptTemplates::load(&root).expect_err("invalid templates");
        assert!(
            err.to_string()
                .contains("relationship user prompt uses unknown placeholder: unknown")
        );
    }

    #[test]
    fn rejects_missing_prompt_file() {
        let root = temp_dir("prompt-template-missing-file");
        fs::create_dir_all(&root).expect("dir");
        fs::write(root.join("entity.system.txt"), "Entity system.").expect("file");
        fs::write(root.join("entity.user.txt"), "Entity input: {{input_text}}").expect("file");
        fs::write(root.join("relationship.system.txt"), "Relationship system.").expect("file");

        let err = PromptTemplates::load(&root).expect_err("missing file");
        assert!(err.to_string().contains("failed to read prompt template"));
    }

    #[test]
    fn rejects_empty_template_file() {
        let root = temp_dir("prompt-template-empty");
        write_prompt_templates(
            &root,
            "",
            "Entity input: {{input_text}}",
            "Relationship system.",
            "Relationship input: {{annotated_text}}",
        );

        let err = PromptTemplates::load(&root).expect_err("empty template");
        assert!(err.to_string().contains("entity system prompt is empty"));
    }

    #[test]
    fn rejects_unclosed_placeholder() {
        let root = temp_dir("prompt-template-unclosed");
        write_prompt_templates(
            &root,
            "Entity system.",
            "Entity input: {{input_text}}",
            "Relationship system.",
            "Relationship input: {{annotated_text",
        );

        let err = PromptTemplates::load(&root).expect_err("invalid template");
        assert!(
            err.to_string()
                .contains("relationship user prompt has an unclosed placeholder")
        );
    }

    #[test]
    fn render_rejects_empty_output() {
        let parsed = ParsedTemplate { segments: vec![] };
        let err = parsed.render("test prompt", &[]).expect_err("empty output");
        assert!(
            err.to_string()
                .contains("test prompt rendered to an empty prompt")
        );
    }

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    fn write_prompt_templates(
        root: &Path,
        entity_system: &str,
        entity_user: &str,
        relationship_system: &str,
        relationship_user: &str,
    ) {
        fs::create_dir_all(root).expect("dir");
        fs::write(root.join("entity.system.txt"), entity_system).expect("entity system");
        fs::write(root.join("entity.user.txt"), entity_user).expect("entity user");
        fs::write(root.join("relationship.system.txt"), relationship_system)
            .expect("relationship system");
        fs::write(root.join("relationship.user.txt"), relationship_user)
            .expect("relationship user");
    }
}
