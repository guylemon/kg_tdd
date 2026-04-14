mod adapters;
mod app;
mod application;
mod domain;
#[cfg(test)]
mod gold_fixtures;
mod ports;

use std::process;

use log::debug;

use crate::adapters::{
    CliArgs, ConfiguredSchemaLlmClient, FileDocumentSource, FileGraphArtifactSink,
    HubTokenizerSource,
};
use crate::app::App;

fn main() {
    init_logger();

    match CliArgs::parse() {
        Ok(args) => {
            debug!(
                "parsed CLI args: provider_mode={:?}, input_path={}, output_dir={}",
                args.run_config.provider.mode,
                args.run_config.input_path.display(),
                args.run_config.output_dir.display()
            );
            let llm_client = match ConfiguredSchemaLlmClient::from_config(&args.run_config.provider)
            {
                Ok(client) => client,
                Err(err) => {
                    eprintln!("{err}");
                    process::exit(err.exit_code());
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
                process::exit(err.exit_code());
            }
        }
        Err(err) => {
            eprintln!("{err}");
            process::exit(err.exit_code());
        }
    }
}

fn init_logger() {
    let env = env_logger::Env::default().default_filter_or("off");
    let mut builder = env_logger::Builder::from_env(env);
    let _ = builder.try_init();
}
