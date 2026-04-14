use serde::de::DeserializeOwned;

use log::debug;
use schemars::JsonSchema;

use crate::application::{AppError, ProviderConfig, ProviderMode};
use crate::domain::NonEmptyString;

use super::fixture::FakeSchemaLlmClient;
use super::openai_compatible::OpenAiCompatibleSchemaLlmClient;

pub(crate) trait SchemaLlmClient: Send + Sync {
    fn generate_with_schema<T>(
        &self,
        sys_prompt: NonEmptyString,
        user_prompt: NonEmptyString,
    ) -> Result<T, AppError>
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
    ) -> Result<T, AppError>
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
    use crate::application::{Chunk, ProviderConfig, ProviderMode};
    use crate::domain::{DocumentId, NonEmptyString, TokenCount};

    use super::ConfiguredSchemaLlmClient;
    use crate::adapters::extraction::extract_chunk;

    #[test]
    fn configured_client_uses_fixture_mode_by_default() {
        let client = ConfiguredSchemaLlmClient::from_config(&ProviderConfig {
            mode: ProviderMode::Fixture,
            base_url: None,
            model: None,
        })
        .expect("fixture client");

        let outcome = extract_chunk(
            Chunk {
                document_id: DocumentId(String::from("stdin-document")),
                text: NonEmptyString(String::from("An apple is a red fruit that grows on trees")),
                token_count: TokenCount(10),
            },
            &client,
        )
        .expect("extracts");

        assert_eq!(outcome.entities.len(), 3);
    }
}
