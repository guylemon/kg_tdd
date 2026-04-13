mod domain;
mod ports;
mod adapters;
mod application;
mod app;

use std::{io, process};

use crate::app::App;
use crate::application::MaxConcurrency;
use crate::adapters::FakeSchemaLlmClient;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let input_reader = stdin.lock();
    let cytoscape_writer = stdout.lock();

    let max_concurrency = MaxConcurrency(4);

    let application = App::new(
        input_reader,
        cytoscape_writer,
        max_concurrency,
        FakeSchemaLlmClient,
    );

    if let Err(e) = application.run() {
        eprintln!("{e:?}");
        process::exit(1)
    }
}
