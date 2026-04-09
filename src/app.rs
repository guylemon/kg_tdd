use serde::Serialize;
use std::io::Read;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;

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
    // TODO are these ids only required for cytoscape? If so, rename to CyNodeId.
    id: NodeId,

    /// The canonical name of the entity, produced during entity resolution.
    name: EntityName,

    /// The type of the entity
    entity_type: EntityType,

    /// An LLM generated summary of all entity descriptions collected during entity resolution.
    description: NodeDescription,

    /// A collection of ``TextUnit`` referring to this entity
    mentions: Vec<TextUnit>,
}

/// Newtype to prevent rogue string use
#[derive(Debug, Serialize)]
struct NodeId(String);

/// Newtype to prevent rogue string use
#[derive(Debug, Serialize)]
struct EntityName(String);

/// Newtype to prevent rogue string use
#[derive(Debug, Serialize)]
struct NodeDescription(String);

/// The supported Entity types for this application
#[derive(Debug, Serialize)]
enum EntityType {
    /// Theoretical ideas, methodologies, approaches
    Concept,

    /// Conferences, releases, historical events
    #[allow(unused)]
    Event,

    /// A biological lifeform, such as a plant, animal, insect. Given the current context, it is not a
    /// product.
    #[allow(unused)]
    Lifeform,

    /// Cities, countries, regions
    #[allow(unused)]
    Location,

    /// Companies, institutions, or universities
    #[allow(unused)]
    Organization,

    /// People mentioned in the chunk text
    #[allow(unused)]
    Person,

    /// Software products, platforms, services
    #[allow(unused)]
    Product,

    /// Frameworks, programming languages, algorithms
    #[allow(unused)]
    Technology,
}

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
    weight: EdgeWeight,
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
struct TextUnit {
    /// The id of the source document
    document_id: DocumentId,

    /// The raw chunk text
    text: NonEmptyString,

    /// The number of tokens in the raw chunk text
    /// ``token_count`` can be used to control context size in LLM prompts containing the
    /// ``TextUnit``
    token_count: TokenCount,
}

#[derive(Debug, Serialize)]
struct DocumentId(String);

#[derive(Debug, Serialize)]
struct NonEmptyString(String);

#[derive(Debug, Serialize)]
struct TokenCount(usize);

/// Represents a unit of source material for the graph produced by this application
#[allow(unused)]
struct Document {
    // TODO define how this is derived
    id: DocumentId,

    /// The full raw text of the document
    text: NonEmptyString,
}

struct KnowledgeGraph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

struct GraphNode;
struct GraphEdge;

struct EntityMention;
struct RelationshipMention;

// TODO remove public field and create initializer
pub struct MaxConcurrency(pub u8);

/// A unit of work in the map reduce phase of the ingestion pipeline
// TODO determine whether to add a task type enum
struct ExtractionTask {
    chunk: Chunk,
}

struct ExtractionResult {
    entities: Vec<EntityMention>,
    relationships: Vec<RelationshipMention>,
}

// Type alias for an unprocessed chunk. A text unit has its entities marked.
type Chunk = TextUnit;

pub struct App<R, W> {
    input_reader: R,
    cytoscape_writer: W,
    /// The maximum number of threads to use when performing concurrent workloads during ingestion
    max_concurrency: MaxConcurrency,
}

impl<R, W> App<R, W>
where
    R: Read,
    W: Write,
{
    pub fn new(input_reader: R, cytoscape_writer: W, max_concurrency: MaxConcurrency) -> Self {
        Self {
            input_reader,
            cytoscape_writer,
            max_concurrency,
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

        let document = Document {
            id: DocumentId(TODO_STRING.to_owned()),
            text: NonEmptyString(raw_text),
        };

        let chunks = partition_document(&document);

        let (entity_mentions, relationship_mentions) =
            map_reduce_extract(&self.max_concurrency, chunks)?;

        let (nodes, edges): (Vec<GraphNode>, Vec<GraphEdge>) =
            finalize_entities_relationships(entity_mentions, relationship_mentions);

        let knowledge_graph = assemble_graph(nodes, edges);

        let cytoscape_elements = convert_kg_to_cytoscape_elements(knowledge_graph);
        let cytoscape_json = serde_json::to_string(&cytoscape_elements).map_err(|_| Todo)?;

        // - Write the cytoscape graph
        self.cytoscape_writer
            .write(cytoscape_json.as_bytes())
            .map_err(|_| Todo)?;

        Ok(())
    }
}

fn map_reduce_extract(
    max_concurrency: &MaxConcurrency,
    chunks: Vec<Chunk>,
) -> Result<(Vec<EntityMention>, Vec<RelationshipMention>), Todo> {
    // Cache the number of document chunks
    let num_chunks = chunks.len();

    // TODO evaluate whether it makes sense to use multiple task queues or to run map within a
    // single queue
    // General task FIFO queue with exclusive receiver
    let (task_tx, task_rx) = mpsc::channel::<ExtractionTask>();
    let task_rx = Arc::new(Mutex::new(task_rx));

    // Extraction result FIFO queue
    let (result_tx, result_rx) = mpsc::channel::<ExtractionResult>();

    let mut handles: Vec<std::thread::JoinHandle<()>> =
        Vec::with_capacity(max_concurrency.0.into());

    // Spawn worker threads
    for _ in 0..max_concurrency.0 {
        let task_receiver = Arc::clone(&task_rx);
        let result_transmitter = result_tx.clone();

        let handle = std::thread::spawn(move || {
            loop {
                let task = {
                    let guard = task_receiver.lock().unwrap();
                    guard.recv()
                };

                match task {
                    Ok(task) => {
                        let marked: TextUnit = mark_entities(task.chunk);
                        let entities: Vec<EntityMention> = extract_entities(&marked);
                        let relationships: Vec<RelationshipMention> = extract_relationships(marked);

                        // Disregard TX error
                        let _ = result_transmitter.send(ExtractionResult {
                            entities,
                            relationships,
                        });
                    }
                    Err(_) => break,
                }
            }
        });
        handles.push(handle);
    }

    // Load task queue
    for chunk in chunks {
        task_tx.send(ExtractionTask { chunk }).map_err(|_| Todo)?;
    }

    // Pull exactly num_chunks results from the results channel
    let mut results = Vec::with_capacity(num_chunks);
    for _ in 0..num_chunks {
        let result = result_rx.recv().map_err(|_| Todo)?;
        results.push(result);
    }

    // Reduce results to tuple
    let aggregated_results = results.into_iter().fold(
        (Vec::new(), Vec::new()), // Accumulated entities and relationships
        |(mut acc_entities, mut acc_relationships), item| {
            acc_entities.extend(item.entities);
            acc_relationships.extend(item.relationships);
            (acc_entities, acc_relationships)
        },
    );

    Ok(aggregated_results)
}

fn extract_relationships(_text_unit: TextUnit) -> Vec<RelationshipMention> {
    // TODO LLM process needed here
    vec![]
}

fn extract_entities(_text_unit: &TextUnit) -> Vec<EntityMention> {
    // TODO LLM process needed here
    vec![]
}

fn mark_entities(chunk: Chunk) -> TextUnit {
    // TODO LLM process needed here
    TextUnit {
        document_id: chunk.document_id,
        text: chunk.text,
        token_count: chunk.token_count,
    }
}

/// Deduplicate and resolve entities and relationships
fn finalize_entities_relationships(
    _entities: Vec<EntityMention>,
    _relationships: Vec<RelationshipMention>,
) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    (vec![], vec![])
}

fn assemble_graph(nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) -> KnowledgeGraph {
    KnowledgeGraph { nodes, edges }
}

fn partition_document(_document: &Document) -> Vec<TextUnit> {
    vec![]
}

/// Constructs the elements structure that cytoscape renders.
// TODO Ultimately, this function should transform a core graph structure for use with cytoscape.
fn convert_kg_to_cytoscape_elements(_kg: KnowledgeGraph) -> CytoscapeElements {
    let cy_node = CyNode {
        data: CyNodeData {
            id: NodeId(TODO_STRING.to_owned()),
            description: NodeDescription(TODO_STRING.to_owned()),
            name: EntityName(TODO_STRING.to_owned()),
            entity_type: EntityType::Concept,
            mentions: vec![TextUnit {
                document_id: DocumentId(TODO_STRING.to_owned()),
                text: NonEmptyString(TODO_STRING.to_owned()),
                token_count: TokenCount(1),
            }],
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
                citation: TextUnit {
                    document_id: DocumentId(TODO_STRING.to_owned()),
                    text: NonEmptyString(TODO_STRING.to_owned()),
                    token_count: TokenCount(1),
                },
                status: EpistemicStatus::Arbitrary,
            }],
            weight: EdgeWeight(Todo),
        },
    };

    CytoscapeElements {
        nodes: vec![cy_node],
        edges: vec![cy_edge],
    }
}
