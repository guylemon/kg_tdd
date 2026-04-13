use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) enum AppError {
    Usage(String),
    InvalidInputPath(PathBuf),
    ReadInput { path: PathBuf },
    EmptyInput { path: PathBuf },
    LoadTokenizer(String),
    InvalidProviderConfig(String),
    ProviderTransport(String),
    ProviderTimeout(String),
    ProviderAuthentication(String),
    ProviderResponse(String),
    ExtractChunk,
    ProjectGraph,
    CreateOutputDir { path: PathBuf },
    WriteOutput { path: PathBuf },
}

impl AppError {
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::InvalidInputPath(_) => 3,
            Self::ReadInput { .. } | Self::EmptyInput { .. } => 4,
            Self::LoadTokenizer(_) => 5,
            Self::InvalidProviderConfig(_) => 6,
            Self::ProviderTransport(_)
            | Self::ProviderTimeout(_)
            | Self::ProviderAuthentication(_)
            | Self::ProviderResponse(_)
            | Self::ExtractChunk => 7,
            Self::ProjectGraph => 8,
            Self::CreateOutputDir { .. } | Self::WriteOutput { .. } => 9,
        }
    }

    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    pub(crate) fn invalid_input_path(path: impl Into<PathBuf>) -> Self {
        Self::InvalidInputPath(path.into())
    }

    pub(crate) fn read_input(path: impl Into<PathBuf>) -> Self {
        Self::ReadInput { path: path.into() }
    }

    pub(crate) fn empty_input(path: impl Into<PathBuf>) -> Self {
        Self::EmptyInput { path: path.into() }
    }

    pub(crate) fn load_tokenizer(name: impl Into<String>) -> Self {
        Self::LoadTokenizer(name.into())
    }

    pub(crate) fn invalid_provider_config(message: impl Into<String>) -> Self {
        Self::InvalidProviderConfig(message.into())
    }

    pub(crate) fn provider_transport(message: impl Into<String>) -> Self {
        Self::ProviderTransport(message.into())
    }

    pub(crate) fn provider_timeout(message: impl Into<String>) -> Self {
        Self::ProviderTimeout(message.into())
    }

    pub(crate) fn provider_authentication(message: impl Into<String>) -> Self {
        Self::ProviderAuthentication(message.into())
    }

    pub(crate) fn provider_response(message: impl Into<String>) -> Self {
        Self::ProviderResponse(message.into())
    }

    pub(crate) fn create_output_dir(path: impl Into<PathBuf>) -> Self {
        Self::CreateOutputDir { path: path.into() }
    }

    pub(crate) fn write_output(path: impl Into<PathBuf>) -> Self {
        Self::WriteOutput { path: path.into() }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "{message}"),
            Self::InvalidInputPath(path) => {
                write!(
                    f,
                    "input path is invalid or does not exist: {}",
                    display_path(path)
                )
            }
            Self::ReadInput { path } => {
                write!(f, "failed to read input file: {}", display_path(path))
            }
            Self::EmptyInput { path } => {
                write!(f, "input file is empty: {}", display_path(path))
            }
            Self::LoadTokenizer(name) => write!(f, "failed to load tokenizer: {name}"),
            Self::InvalidProviderConfig(message) => {
                write!(f, "invalid provider configuration: {message}")
            }
            Self::ProviderTransport(message) => {
                write!(f, "provider transport failure: {message}")
            }
            Self::ProviderTimeout(message) => write!(f, "provider timeout: {message}"),
            Self::ProviderAuthentication(message) => {
                write!(f, "provider authentication failure: {message}")
            }
            Self::ProviderResponse(message) => {
                write!(f, "invalid provider response: {message}")
            }
            Self::ExtractChunk => write!(f, "failed to extract entities or relationships"),
            Self::ProjectGraph => write!(f, "failed to serialize graph output"),
            Self::CreateOutputDir { path } => {
                write!(
                    f,
                    "failed to create output directory: {}",
                    display_path(path)
                )
            }
            Self::WriteOutput { path } => {
                write!(f, "failed to write output file: {}", display_path(path))
            }
        }
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
