use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tokenizers::Tokenizer;

use crate::application::AppError;

pub(crate) trait TokenizerSource: Send + Sync {
    fn load(&self, tokenizer_name: &str) -> Result<Tokenizer, AppError>;
}

pub(crate) struct HubTokenizerSource;

impl TokenizerSource for HubTokenizerSource {
    fn load(&self, tokenizer_name: &str) -> Result<Tokenizer, AppError> {
        static TOKENIZER_CACHE: OnceLock<Mutex<HashMap<String, Tokenizer>>> = OnceLock::new();
        let cache = TOKENIZER_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

        if let Some(tokenizer) = cache
            .lock()
            .map_err(|_| AppError::LoadTokenizer)?
            .get(tokenizer_name)
            .cloned()
        {
            return Ok(tokenizer);
        }

        let tokenizer = Tokenizer::from_pretrained(tokenizer_name, None)
            .map_err(|_| AppError::LoadTokenizer)?;

        cache
            .lock()
            .map_err(|_| AppError::LoadTokenizer)?
            .insert(tokenizer_name.to_owned(), tokenizer.clone());

        Ok(tokenizer)
    }
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct StaticTokenizerSource {
    tokenizer: Tokenizer,
}

#[cfg(test)]
impl StaticTokenizerSource {
    pub(crate) fn new(tokenizer: Tokenizer) -> Self {
        Self { tokenizer }
    }
}

#[cfg(test)]
impl TokenizerSource for StaticTokenizerSource {
    fn load(&self, _tokenizer_name: &str) -> Result<Tokenizer, AppError> {
        Ok(self.tokenizer.clone())
    }
}
