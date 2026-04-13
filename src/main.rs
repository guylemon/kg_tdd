mod adapters;
mod app;
mod application;
mod domain;
mod ports;

use std::{io, process};

use crate::adapters::{FakeSchemaLlmClient, HubTokenizerSource};
use crate::app::App;
use crate::application::{IngestConfig, MaxConcurrency};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let input_reader = stdin.lock();
    let cytoscape_writer = stdout.lock();

    let config = IngestConfig::default();
    let max_concurrency = MaxConcurrency(4);

    let application = App::new(
        config,
        input_reader,
        cytoscape_writer,
        max_concurrency,
        FakeSchemaLlmClient,
        HubTokenizerSource,
    );

    if let Err(e) = application.run() {
        eprintln!("{e:?}");
        process::exit(1)
    }
}
