use std::io::Read;

#[derive(Debug)]
pub struct Todo;

pub struct App<R> {
    input_reader: R,
}

impl<R> App<R>
where
    R: Read,
{
    pub fn new(input_reader: R) -> Self {
        Self { input_reader }
    }

    pub fn run(mut self) -> Result<Todo, Todo> {
        // Read the raw source document text. The application will create a graph structure from
        // the entities contained within the text.
        // TODO do something with this value after output sketch is ready
        let mut raw_text = String::new();
        self.input_reader
            .read_to_string(&mut raw_text)
            .map_err(|_| Todo)?;

        // The application will transform the graph to a JSON format used by Cytoscape.js and write
        // it to a file for inclusion in a static html document. https://js.cytoscape.org/#demos
        Ok(Todo)
    }
}
