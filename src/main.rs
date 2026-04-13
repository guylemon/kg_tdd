mod adapters;
mod app;
mod application;
mod domain;
mod ports;

use std::process;

use crate::adapters::{
    CliArgs, FakeSchemaLlmClient, FileDocumentSource, FileGraphArtifactSink, HubTokenizerSource,
};
use crate::app::App;

fn main() {
    match CliArgs::parse() {
        Ok(args) => {
            let application = App::new(
                args.run_config,
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
