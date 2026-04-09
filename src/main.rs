mod app;

use std::{io::Cursor, process};

use crate::app::App;

fn main() {
    // TODO use stdin in near future. To support type-driven development, use this stub type.
    let input_reader = Cursor::new(Vec::new());
    let application = App::new(input_reader);

    // TODO no need to use debug when an error type is in place.
    if let Err(e) = application.run() {
        eprintln!("{e:?}");
        process::exit(1)
    }
    println!("Hello, world!");
}
