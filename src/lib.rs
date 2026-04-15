mod adapters;
mod app;
mod application;
mod domain;
pub mod eval_support;
mod ports;

use tracing::debug;
use tracing_subscriber::{EnvFilter, prelude::*};

use crate::adapters::{
    CliArgs, ConfiguredSchemaLlmClient, FileDocumentSource, FileGraphArtifactSink,
    HubTokenizerSource,
};
use crate::app::App;

#[must_use]
pub fn run_cli() -> i32 {
    init_tracing_for_process();

    match CliArgs::parse() {
        Ok(args) => {
            debug!(
                provider_mode = ?args.run_config.provider.mode,
                input_path = %args.run_config.input_path.display(),
                output_dir = %args.run_config.output_dir.display(),
                "parsed CLI args"
            );
            let llm_client = match ConfiguredSchemaLlmClient::from_config(&args.run_config.provider)
            {
                Ok(client) => client,
                Err(err) => {
                    eprintln!("{err}");
                    return err.exit_code();
                }
            };
            let application = App::new(
                args.run_config,
                FileDocumentSource,
                FileGraphArtifactSink,
                llm_client,
                HubTokenizerSource,
            );

            if let Err(err) = application.run() {
                eprintln!("{err}");
                return err.exit_code();
            }

            0
        }
        Err(err) => {
            eprintln!("{err}");
            err.exit_code()
        }
    }
}

pub fn init_tracing_for_process() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
    let subscriber = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .json()
            .with_writer(std::io::stderr),
    );
    let _ = subscriber.try_init();
}
