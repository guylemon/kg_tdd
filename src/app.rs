use serde::Serialize;
use std::io::Read;
use std::io::Write;

const TODO_STRING: &str = "";

#[derive(Debug)]
pub struct Todo;

/// A serializable type used as input to the cytoscape JavaScript UI library.
/// See more: <https://js.cytoscape.org/#notation/elements-json>
#[derive(Debug, Serialize)]
struct CytoscapeElements {
    /// The cytoscape graph nodes
    nodes: Vec<CyNode>,
    /// The cytoscape graph edges
    edges: Vec<CyEdge>,
}

#[derive(Debug, Serialize)]
struct CyNode {
    data: CyNodeData,
}

#[derive(Debug, Serialize)]
struct CyEdge {
    data: CyEdgeData,
}

#[derive(Debug, Serialize)]
struct CyNodeData {
    id: String,
}

#[derive(Debug, Serialize)]
struct CyEdgeData {
    id: String,
    source: String,
    target: String,
}

pub struct App<R, W> {
    input_reader: R,
    cytoscape_writer: W,
}

impl<R, W> App<R, W>
where
    R: Read,
    W: Write,
{
    pub fn new(input_reader: R, cytoscape_writer: W) -> Self {
        Self {
            input_reader,
            cytoscape_writer,
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

        let elements = cytoscape_elements();

        // TODO this may end up being a thin wrapper around serde, in which case it will be
        // replaced later. For now, we are avoiding implementation details.
        let cytoscape_json = serialize_cy_elements(&elements)?;

        // Write the serialized graph
        // TODO replace TODO_STRING with a serialized JSON graph in the proper format for Cytoscape.
        self.cytoscape_writer
            .write(cytoscape_json.as_bytes())
            .map_err(|_| Todo)?;

        Ok(())
    }
}

/// Constructs the elements structure that cytoscape renders.
// TODO Ultimately, this function should transform a core graph structure for use with cytoscape.
// While working backward to discover the domain, stub the elements as needed.
fn cytoscape_elements() -> CytoscapeElements {
    let cy_node = CyNode {
        data: CyNodeData {
            id: TODO_STRING.to_owned(),
        },
    };

    let cy_edges = CyEdge {
        data: CyEdgeData {
            id: TODO_STRING.to_owned(),
            source: TODO_STRING.to_owned(),
            target: TODO_STRING.to_owned(),
        },
    };

    CytoscapeElements {
        nodes: vec![cy_node],
        edges: vec![cy_edges],
    }
}

// TODO this may end up being a thin wrapper around serde, in which case it will be
// replaced later. For now, we are avoiding implementation details.
fn serialize_cy_elements(cy_elements: &CytoscapeElements) -> Result<String, Todo> {
    serde_json::to_string(cy_elements).map_err(|_| Todo)
}
