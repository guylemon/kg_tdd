use std::io::Read;
use std::io::Write;

const TODO_STRING: &str = "";

#[derive(Debug)]
pub struct Todo;

pub struct CytoscapeGraph;

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

        // TODO this may end up being a thin wrapper around serde, in which case it will be
        // replaced later. For now, we are avoiding implementation details.
        let cytoscape_json = serialize_graph_cy(CytoscapeGraph);

        // Write the serialized graph
        // TODO replace TODO_STRING with a serialized JSON graph in the proper format for Cytoscape.
        self.graph_writer
            .write(cytoscape_json.as_bytes())
            .map_err(|_| Todo)?;

        Ok(())
    }
}

// TODO this may end up being a thin wrapper around serde, in which case it will be
// replaced later. For now, we are avoiding implementation details.
fn serialize_graph_cy(_cy_graph: CytoscapeGraph) -> String {
    // TODO replace TODO_STRING with a serialized JSON graph in the proper format for Cytoscape.
    TODO_STRING.to_owned()
}
