use serde::Serialize;
use std::io::Read;
use std::io::Write;

const TODO_STRING: &str = "";

#[derive(Debug, Serialize)]
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
    id: NodeId,
}

/// Newtype to prevent rogue string use
#[derive(Debug, Serialize)]
struct NodeId(String);

#[derive(Debug, Serialize)]
struct CyEdgeData {
    id: EdgeId,

    /// The source node identifier
    source: NodeId,

    /// The target node identifier
    target: NodeId,

    /// An LLM generated summary of the relationship between the source and target nodes
    description: EdgeDescription,

    /// Evidence from the source text that substantiates the relationship between the source and
    /// target nodes.
    evidence: Vec<FactualClaim>,

    /// The weight of the relationship, derived from the strength of the `evidence`
    weight: EdgeWeight
}

// TODO It is unclear which type of number to use at this time.
#[derive(Debug, Serialize)]
struct EdgeWeight(Todo);

/// Newtype to prevent rogue string use
#[derive(Debug, Serialize)]
struct EdgeId(String);

/// Newtype to prevent rogue string use
#[derive(Debug, Serialize)]
struct EdgeDescription(String);

/// A factual claim that supports a proposed relationship between a source and target node.
#[derive(Debug, Serialize)]
struct FactualClaim {
    /// The factual claim
    fact: Fact,

    /// The source text unit
    // TODO should this be a reference or owned?
    citation: TextUnit,

    /// The degree of confidence in the claim.
    status: EpistemicStatus,
}

/// Represents a single chunk of a source document.
#[derive(Debug, Serialize)]
struct Fact(String);

/// Represents the degree of epistemic certainty for a given claim, given the context.
#[derive(Debug, Serialize)]
enum EpistemicStatus {
    /// An arbitrary claim has no perceptual or conceptual evidence. It is neither true or false
    /// because it is outside of cognition. The arbitrary is distinct from the possible because the
    /// arbitrary has no proposed evidence.
    ///
    /// Examples:
    /// - "There is a teapot orbiting Mars."
    /// - "You are secretly a parrot."
    Arbitrary,

    /// A claim for which there is some, but not conclusive evidence. There is nothing in the
    /// current context of knowledge that contradicts the evidence.
    #[allow(unused)]
    Possible,

    /// A claim for which the evidence is strong, but not yet conclusive. The context indicates
    /// that the weight of the evidence supports the claim, making it more likely to be true than
    /// false; however, further evidence could still tip the scale.
    #[allow(unused)]
    Probable,

    /// A claim for which the evidence is conclusive within a given context of knowledge. All
    /// available evidence supports the claim, and there is no evidence to support any alternative.
    #[allow(unused)]
    Certain,
}

/// Represents a single chunk of a source document.
#[derive(Debug, Serialize)]
struct TextUnit;

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
            id: NodeId(TODO_STRING.to_owned()),
        },
    };

    let cy_edge = CyEdge {
        data: CyEdgeData {
            id: EdgeId(TODO_STRING.to_owned()),
            source: NodeId(TODO_STRING.to_owned()),
            target: NodeId(TODO_STRING.to_owned()),
            description: EdgeDescription(TODO_STRING.to_owned()),
            evidence: vec![FactualClaim {
                fact: Fact(TODO_STRING.to_owned()),
                citation: TextUnit,
                status: EpistemicStatus::Arbitrary,
            }],
            weight: EdgeWeight(Todo)
        },
    };

    CytoscapeElements {
        nodes: vec![cy_node],
        edges: vec![cy_edge],
    }
}

// TODO this may end up being a thin wrapper around serde, in which case it will be
// replaced later. For now, we are avoiding implementation details.
fn serialize_cy_elements(cy_elements: &CytoscapeElements) -> Result<String, Todo> {
    serde_json::to_string(cy_elements).map_err(|_| Todo)
}
