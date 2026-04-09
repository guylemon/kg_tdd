mod app;

use std::{io::Cursor, process};

use crate::app::App;

fn main() {
    // TODO use stdin in near future. To support type-driven development, use this stub type.
    let input_reader = Cursor::new(Vec::new());

    // TODO use a file in the near future. The application will transform the graph to a JSON
    // format used by Cytoscape.js and write it to a file for inclusion in a static html document.
    // https://js.cytoscape.org/#demos
    let graph_writer = Cursor::new(Vec::new());

    let application = App::new(input_reader, graph_writer);

    // TODO no need to use debug when an error type is in place.
    if let Err(e) = application.run() {
        eprintln!("{e:?}");
        process::exit(1)
    }
    println!("Hello, world!");
}
