mod adapters;
mod app;
mod application;
mod domain;
mod ports;

use std::process;

use crate::adapters::{CliArgs, FakeSchemaLlmClient, FileDocumentSource, FileGraphArtifactSink, HubTokenizerSource};
use crate::app::App;
use crate::application::MaxConcurrency;

fn main() {
    match CliArgs::parse() {
        Ok(args) => {
            let application = App::new(
                args.ingest_config,
                args.input_path,
                args.output_dir,
                MaxConcurrency(4),
                FileDocumentSource,
                FileGraphArtifactSink,
                FakeSchemaLlmClient,
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
