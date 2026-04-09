use std::io::Read;
use std::io::Write;

const TODO_STRING: &str = "";

#[derive(Debug)]
pub struct Todo;

pub struct App<R, W> {
    input_reader: R,
    graph_writer: W,
}

impl<R, W> App<R, W>
where
    R: Read,
    W: Write,
{
    pub fn new(input_reader: R, graph_writer: W) -> Self {
        Self {
            input_reader,
            graph_writer,
        }
    }

    pub fn run(mut self) -> Result<(), Todo> {
        // Read the raw source document text. The application will create a graph structure from
        // the entities contained within the text.
        // TODO do something with this value after output sketch is ready
        let mut raw_text = String::new();
        self.input_reader
            .read_to_string(&mut raw_text)
            .map_err(|_| Todo)?;

        // Write the serialized graph
        self.graph_writer
            .write(TODO_STRING.as_bytes())
            .map_err(|_| Todo)?;

        Ok(())
    }
}
