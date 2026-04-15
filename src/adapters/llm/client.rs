use serde::de::DeserializeOwned;

use schemars::JsonSchema;
use tracing::debug;

use crate::application::{AppError, ProviderConfig, ProviderMode};
use crate::domain::NonEmptyString;

use super::fixture::FakeSchemaLlmClient;
use super::openai_compatible::OpenAiCompatibleSchemaLlmClient;

pub(crate) struct GeneratedSchemaValue<T> {
    pub(crate) parsed: T,
    pub(crate) raw_response: String,
}

pub(crate) trait SchemaLlmClient: Send + Sync {
    fn generate_with_schema<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<GeneratedSchemaValue<T>, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static;
}

pub(crate) enum ConfiguredSchemaLlmClient {
    Fixture(FakeSchemaLlmClient),
    OpenAiCompatible(OpenAiCompatibleSchemaLlmClient),
}

impl ConfiguredSchemaLlmClient {
    pub(crate) fn from_config(config: &ProviderConfig) -> Result<Self, AppError> {
        debug!(
            "configuring schema LLM client for provider_mode={:?}",
            config.mode
        );
        match config.mode {
            ProviderMode::Fixture => Ok(Self::Fixture(FakeSchemaLlmClient)),
            ProviderMode::OpenAiCompatible => Ok(Self::OpenAiCompatible(
                OpenAiCompatibleSchemaLlmClient::from_config(config)?,
            )),
        }
    }
}

impl SchemaLlmClient for ConfiguredSchemaLlmClient {
    fn generate_with_schema<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<GeneratedSchemaValue<T>, AppError>
    where
        T: DeserializeOwned + JsonSchema + 'static,
    {
        match self {
            Self::Fixture(client) => client.generate_with_schema(sys_prompt, user_prompt),
            Self::OpenAiCompatible(client) => client.generate_with_schema(sys_prompt, user_prompt),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::application::{Chunk, ProviderConfig, ProviderMode};
    use crate::domain::{DocumentId, NonEmptyString, TokenCount};

    use super::ConfiguredSchemaLlmClient;
    use crate::adapters::extraction::extract_chunk;
    use crate::adapters::prompt::PromptTemplates;

    #[test]
    fn configured_client_uses_fixture_mode_by_default() {
        let client = ConfiguredSchemaLlmClient::from_config(&ProviderConfig {
            mode: ProviderMode::Fixture,
            base_url: None,
            model: None,
        })
        .expect("fixture client");
        let prompt_templates = load_prompt_templates();

        let outcome = extract_chunk(
            Chunk {
                document_id: DocumentId(String::from("stdin-document")),
                text: NonEmptyString(String::from("An apple is a red fruit that grows on trees")),
                token_count: TokenCount(10),
            },
            &client,
            &prompt_templates,
        )
        .expect("extracts");

        assert_eq!(outcome.entities.len(), 3);
    }

    fn load_prompt_templates() -> PromptTemplates {
        let dir = std::env::temp_dir().join(format!(
            "prompt-templates-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("entity.system.txt"), "Entity system prompt.").expect("entity system");
        fs::write(dir.join("entity.user.txt"), "{{input_text}}").expect("entity user");
        fs::write(
            dir.join("relationship.system.txt"),
            "Relationship system prompt.",
        )
        .expect("relationship system");
        fs::write(dir.join("relationship.user.txt"), "{{annotated_text}}")
            .expect("relationship user");

        PromptTemplates::load(Path::new(&dir)).expect("prompt templates")
    }
}
