mod adapters;
mod app;
mod application;
mod domain;
mod ports;

use std::process;

use crate::adapters::{
    CliArgs, ConfiguredSchemaLlmClient, FileDocumentSource, FileGraphArtifactSink,
    HubTokenizerSource,
};
use crate::app::App;

fn main() {
    match CliArgs::parse() {
        Ok(args) => {
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
