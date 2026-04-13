use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::application::{
    AppError, IngestConfig, MaxConcurrency, ProviderConfig, ProviderMode, RunConfig,
};

#[derive(Debug)]
pub(crate) struct CliArgs {
    pub(crate) run_config: RunConfig,
}

impl CliArgs {
    pub(crate) fn parse() -> Result<Self, AppError> {
        Self::parse_from(env::args_os().skip(1))
    }

    fn parse_from<I>(args: I) -> Result<Self, AppError>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut input_path = None;
        let mut output_dir = None;
        let mut tokenizer_name = None;
        let mut max_chunk_tokens = None;
        let mut provider_mode = None;
        let mut provider_base_url = None;
        let mut provider_model = None;

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_string_lossy().as_ref() {
                "--help" | "-h" => return Err(AppError::usage(usage())),
                "--input" => set_path_flag("--input", &mut input_path, args.next())?,
                "--output-dir" => set_path_flag("--output-dir", &mut output_dir, args.next())?,
                "--tokenizer" => set_string_flag("--tokenizer", &mut tokenizer_name, args.next())?,
                "--max-chunk-tokens" => {
                    set_usize_flag("--max-chunk-tokens", &mut max_chunk_tokens, args.next())?;
                }
                "--provider-mode" => {
                    let mode =
                        next_string_value("--provider-mode", args.next()).and_then(parse_mode)?;
                    set_parsed_flag("--provider-mode", &mut provider_mode, mode)?;
                }
                "--provider-base-url" => {
                    set_string_flag("--provider-base-url", &mut provider_base_url, args.next())?;
                }
                "--provider-model" => {
                    set_string_flag("--provider-model", &mut provider_model, args.next())?;
                }
                other => {
                    return Err(AppError::usage(format!(
                        "unknown argument: {other}\n\n{}",
                        usage()
                    )));
                }
            }
        }

        let input_path = input_path.ok_or_else(|| {
            AppError::usage(format!("missing required flag: --input\n\n{}", usage()))
        })?;
        let output_dir = output_dir.ok_or_else(|| {
            AppError::usage(format!(
                "missing required flag: --output-dir\n\n{}",
                usage()
            ))
        })?;

        Ok(Self {
            run_config: RunConfig {
                ingest: IngestConfig {
                    tokenizer_name: tokenizer_name
                        .unwrap_or_else(|| String::from("bert-base-cased")),
                    max_chunk_tokens: max_chunk_tokens.unwrap_or(128),
                },
                input_path,
                output_dir,
                max_concurrency: MaxConcurrency(4),
                provider: ProviderConfig {
                    mode: provider_mode.unwrap_or(ProviderMode::Fixture),
                    base_url: provider_base_url,
                    model: provider_model,
                },
            },
        })
    }
}

fn set_path_flag(
    flag: &str,
    slot: &mut Option<PathBuf>,
    value: Option<OsString>,
) -> Result<(), AppError> {
    if slot.is_some() {
        return Err(AppError::usage(format!(
            "duplicate flag: {flag}\n\n{}",
            usage()
        )));
    }

    let value =
        value.ok_or_else(|| AppError::usage(format!("missing value for {flag}\n\n{}", usage())))?;
    *slot = Some(PathBuf::from(value));
    Ok(())
}

fn set_string_flag(
    flag: &str,
    slot: &mut Option<String>,
    value: Option<OsString>,
) -> Result<(), AppError> {
    if slot.is_some() {
        return Err(AppError::usage(format!(
            "duplicate flag: {flag}\n\n{}",
            usage()
        )));
    }

    let value =
        value.ok_or_else(|| AppError::usage(format!("missing value for {flag}\n\n{}", usage())))?;
    *slot = Some(value.to_string_lossy().into_owned());
    Ok(())
}

fn set_usize_flag(
    flag: &str,
    slot: &mut Option<usize>,
    value: Option<OsString>,
) -> Result<(), AppError> {
    if slot.is_some() {
        return Err(AppError::usage(format!(
            "duplicate flag: {flag}\n\n{}",
            usage()
        )));
    }

    let value =
        value.ok_or_else(|| AppError::usage(format!("missing value for {flag}\n\n{}", usage())))?;
    let raw = value.to_string_lossy();
    let parsed = raw
        .parse::<usize>()
        .map_err(|_| AppError::usage(format!("invalid value for {flag}: {raw}\n\n{}", usage())))?;
    *slot = Some(parsed);
    Ok(())
}

fn set_parsed_flag<T>(flag: &str, slot: &mut Option<T>, value: T) -> Result<(), AppError> {
    if slot.is_some() {
        return Err(AppError::usage(format!(
            "duplicate flag: {flag}\n\n{}",
            usage()
        )));
    }

    *slot = Some(value);
    Ok(())
}

fn next_string_value(flag: &str, value: Option<OsString>) -> Result<String, AppError> {
    value
        .map(|item| item.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::usage(format!("missing value for {flag}\n\n{}", usage())))
}

fn parse_mode(raw: String) -> Result<ProviderMode, AppError> {
    match raw.as_str() {
        "fixture" => Ok(ProviderMode::Fixture),
        "openai-compatible" => Ok(ProviderMode::OpenAiCompatible),
        _ => Err(AppError::usage(format!(
            "invalid value for --provider-mode: {raw}\n\n{}",
            usage()
        ))),
    }
}

fn usage() -> &'static str {
    "Usage: cargo run -- --input <PATH> --output-dir <PATH> [--tokenizer <NAME>] [--max-chunk-tokens <N>] [--provider-mode <fixture|openai-compatible>] [--provider-base-url <URL>] [--provider-model <NAME>]"
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::CliArgs;

    #[test]
    fn parses_required_and_optional_flags() {
        let args = vec![
            OsString::from("--input"),
            OsString::from("input.txt"),
            OsString::from("--output-dir"),
            OsString::from("out"),
            OsString::from("--tokenizer"),
            OsString::from("bert-base-uncased"),
            OsString::from("--max-chunk-tokens"),
            OsString::from("64"),
            OsString::from("--provider-mode"),
            OsString::from("openai-compatible"),
            OsString::from("--provider-base-url"),
            OsString::from("http://localhost:8080"),
            OsString::from("--provider-model"),
            OsString::from("llama3.2"),
        ];

        let parsed = CliArgs::parse_from(args).expect("cli args");

        assert_eq!(parsed.run_config.input_path, PathBuf::from("input.txt"));
        assert_eq!(parsed.run_config.output_dir, PathBuf::from("out"));
        assert_eq!(parsed.run_config.ingest.tokenizer_name, "bert-base-uncased");
        assert_eq!(parsed.run_config.ingest.max_chunk_tokens, 64);
        assert_eq!(
            parsed.run_config.provider.mode,
            crate::application::ProviderMode::OpenAiCompatible
        );
        assert_eq!(
            parsed.run_config.provider.base_url.as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(
            parsed.run_config.provider.model.as_deref(),
            Some("llama3.2")
        );
    }

    #[test]
    fn uses_default_optional_values() {
        let args = vec![
            OsString::from("--input"),
            OsString::from("input.txt"),
            OsString::from("--output-dir"),
            OsString::from("out"),
        ];

        let parsed = CliArgs::parse_from(args).expect("cli args");

        assert_eq!(parsed.run_config.ingest.tokenizer_name, "bert-base-cased");
        assert_eq!(parsed.run_config.ingest.max_chunk_tokens, 128);
        assert_eq!(
            parsed.run_config.provider.mode,
            crate::application::ProviderMode::Fixture
        );
        assert_eq!(parsed.run_config.provider.base_url, None);
        assert_eq!(parsed.run_config.provider.model, None);
    }

    #[test]
    fn rejects_unknown_flags() {
        let err = CliArgs::parse_from(vec![OsString::from("--wat")]).expect_err("unknown flag");
        assert!(err.to_string().contains("unknown argument: --wat"));
    }

    #[test]
    fn rejects_duplicate_flags() {
        let err = CliArgs::parse_from(vec![
            OsString::from("--input"),
            OsString::from("a.txt"),
            OsString::from("--input"),
            OsString::from("b.txt"),
            OsString::from("--output-dir"),
            OsString::from("out"),
        ])
        .expect_err("duplicate flag");
        assert!(err.to_string().contains("duplicate flag: --input"));
    }

    #[test]
    fn rejects_invalid_numbers() {
        let err = CliArgs::parse_from(vec![
            OsString::from("--input"),
            OsString::from("a.txt"),
            OsString::from("--output-dir"),
            OsString::from("out"),
            OsString::from("--max-chunk-tokens"),
            OsString::from("abc"),
        ])
        .expect_err("invalid number");
        assert!(
            err.to_string()
                .contains("invalid value for --max-chunk-tokens: abc")
        );
    }

    #[test]
    fn rejects_invalid_provider_mode() {
        let err = CliArgs::parse_from(vec![
            OsString::from("--input"),
            OsString::from("a.txt"),
            OsString::from("--output-dir"),
            OsString::from("out"),
            OsString::from("--provider-mode"),
            OsString::from("wat"),
        ])
        .expect_err("invalid provider mode");

        assert!(
            err.to_string()
                .contains("invalid value for --provider-mode: wat")
        );
    }
}
