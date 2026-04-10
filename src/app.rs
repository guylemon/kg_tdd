use serde::Serialize;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
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

// Support efficient transformation to CyNodeData from the GraphNode types
impl From<GraphNode> for CyNodeData {
    fn from(node: GraphNode) -> Self {
        Self {
            id: NodeId(TODO_STRING.to_owned()),
            name: node.name,
            entity_type: node.entity_type,
            description: node.description,
            mentions: node.mentions,
        }
    }
}

// Support efficient transformation to CyNodeData from the GraphNode types
/// Newtype to prevent rogue string use
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct NodeId(String);

/// Newtype to prevent rogue string use
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
struct EntityName(String);

/// Newtype to prevent rogue string use
#[derive(Debug, Serialize)]
struct NodeDescription(String);

/// The supported Entity types for this application
// TODO remove unused allow rule
#[derive(Debug, Serialize)]
enum EntityType {
    /// Theoretical ideas, methodologies, approaches
    #[allow(unused)]
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

impl From<GraphEdge> for CyEdgeData {
    fn from(edge: GraphEdge) -> Self {
        Self {
            id: EdgeId(TODO_STRING.to_owned()),
            source: edge.source,
            target: edge.target,
            description: edge.description,
            evidence: edge.evidence,
            weight: edge.weight,
        }
    }
}

// TODO It is unclear which type of number to use at this time.
#[derive(Debug, Serialize)]
struct EdgeWeight(u16);

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
// TODO remove unused allow rule
#[derive(Debug, Serialize)]
enum EpistemicStatus {
    /// An arbitrary claim has no perceptual or conceptual evidence. It is neither true or false
    /// because it is outside of cognition. The arbitrary is distinct from the possible because the
    /// arbitrary has no proposed evidence.
    ///
    /// Examples:
    /// - "There is a teapot orbiting Mars."
    /// - "You are secretly a parrot."
    #[allow(unused)]
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
    text: AnnotatedText,

    /// The number of tokens in the raw chunk text
    /// ``token_count`` can be used to control context size in LLM prompts containing the
    /// ``TextUnit``
    token_count: TokenCount,
}

/// ``AnnotatedText`` is text from a Chunk that has its entities marked.
///
/// - Raw text -> "I like apples"
/// - Annotated text -> "<entity type=Person>I</entity> like <entity type=Lifeform>apples</entity>"
#[derive(Debug, Serialize)]
struct AnnotatedText(String);

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

struct GraphNode {
    description: NodeDescription,
    entity_type: EntityType,
    mentions: Vec<TextUnit>,
    name: EntityName,
}

struct GraphEdge {
    source: NodeId,
    target: NodeId,
    description: EdgeDescription,
    evidence: Vec<FactualClaim>,
    weight: EdgeWeight,
}

/// An LLM extracted entity from a source ``TextUnit``
struct EntityMention {
    description: NodeDescription,
    entity_type: EntityType,
    name: EntityName,
    /// The source of duplicate ``EntityMentions`` will be folded into the ``GraphNode.mentions``
    /// field.
    source: TextUnit,
}

struct RelationshipMention {
    source: NodeId,
    target: NodeId,
    description: EdgeDescription,
    evidence: Vec<FactualClaim>,
}

// TODO remove public field and create initializer
pub struct MaxConcurrency(pub u8);

/// A unit of work in the map reduce phase of the ingestion pipeline
// TODO determine whether to add a task type enum
#[derive(Debug)]
struct ExtractionTask {
    chunk: Chunk,
}

struct ExtractionResult {
    entities: Vec<EntityMention>,
    relationships: Vec<RelationshipMention>,
}

// Type alias for an unprocessed chunk. A text unit has its entities marked.
#[derive(Debug)]
struct Chunk {
    /// The id of the source document
    document_id: DocumentId,

    /// The raw chunk text
    text: NonEmptyString,

    /// The number of tokens in the raw chunk text
    /// ``token_count`` can be used to control context size in LLM prompts containing the
    /// ``TextUnit``
    token_count: TokenCount,
}

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

        let knowledge_graph = KnowledgeGraph { nodes, edges };
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
    // Identify candidate pairs
    // for max tries
        // Verify whether there are more pairs (Y/N)
        // if Y identify more pairs
        // else break
    // Classify the pairs
    // for max tries, pairs
        // Verify whether classification is correct (Y/N)
        // if N re-classify the entity
        // else break
    // Collect evidence of the relationship
    // for max tries, pairs
        // Verify whether evidence is valid (Y/N)
        // if N re-classify the entity
        // else break
    vec![]
}

fn extract_entities(_text_unit: &TextUnit) -> Vec<EntityMention> {
    // Deterministic extraction from XML tags
    // Determine canonical name (LLM for this?)
    // Deterministic checking (discard invalid)
    vec![]
}

fn mark_entities(chunk: Chunk) -> TextUnit {
    // TODO LLM process needed here
    // Mark the entities
    // for max tries
        // Verify whether there are more entities (Y/N)
        // if Y Mark more entities
        // else break
    // Classify the entities
    // for max tries, entities
        // Verify whether classification is correct (Y/N)
        // if N re-classify the entity
        // else break
    let text = AnnotatedText(chunk.text.0);
    TextUnit {
        document_id: chunk.document_id,
        text,
        token_count: chunk.token_count,
    }
}

/// Deduplicate and resolve entities and relationships
fn finalize_entities_relationships(
    entities: Vec<EntityMention>,
    relationships: Vec<RelationshipMention>,
) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let nodes = consolidate_entities(entities);
    let edges = consolidate_relationships(relationships);

    (nodes, edges)
}

fn consolidate_relationships(relationship_mentions: Vec<RelationshipMention>) -> Vec<GraphEdge> {
    let mut lookup: HashMap<(NodeId, NodeId), GraphEdge> = HashMap::new();

    for mention in relationship_mentions {
        match lookup.entry((mention.source.clone(), mention.target.clone())) {
            Entry::Occupied(mut occupied_entry) => {
                // Add the evidence
                // TODO consider LLM summary or tightening the FactualClaim type so it does not
                // include a full text chunk. The graph will get very heavy when using full text
                // units over several documents. This may be a good feature to add after
                // persistence to a graphdb. For now, accept the heavy nature and move forward.
                occupied_entry.get_mut().evidence.extend(mention.evidence);
                // Increment the weight
                // TODO consider a calculation that increases the weight according to the epistemic
                // status of the underlying evidence.
                occupied_entry.get_mut().weight.0 += 1;
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(GraphEdge {
                    source: mention.source,
                    target: mention.target,
                    description: mention.description,
                    evidence: mention.evidence,
                    weight: EdgeWeight(1),
                });
            }
        }
    }

    lookup.into_values().collect()
}

/// Deduplicates and consolidates ``EntityMentions``
fn consolidate_entities(entity_mentions: Vec<EntityMention>) -> Vec<GraphNode> {
    let mut lookup: HashMap<EntityName, GraphNode> = HashMap::new();

    for mention in entity_mentions {
        // If the mention is in the map by name, add the current mention's source to the GraphNode
        // mentions vector. Otherwise, insert a GraphNode created from the EntityMention.
        match lookup.entry(mention.name.clone()) {
            Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().mentions.push(mention.source);
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(GraphNode {
                    description: mention.description,
                    entity_type: mention.entity_type,
                    mentions: vec![mention.source],
                    name: mention.name,
                });
            }
        }
    }

    lookup.into_values().collect()
}

fn partition_document(_document: &Document) -> Vec<Chunk> {
    vec![]
}

/// Constructs the elements JSON structure that cytoscape requires.
fn convert_kg_to_cytoscape_elements(kg: KnowledgeGraph) -> CytoscapeElements {
    let nodes: Vec<CyNode> = kg
        .nodes
        .into_iter()
        .map(|n| CyNode { data: n.into() })
        .collect();

    let edges: Vec<CyEdge> = kg
        .edges
        .into_iter()
        .map(|e| CyEdge { data: e.into() })
        .collect();

    CytoscapeElements { nodes, edges }
}
