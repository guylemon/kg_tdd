use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tracing::debug;

use crate::adapters::{
    ConfiguredSchemaLlmClient, FileGraphArtifactSink, HubTokenizerSource, ParallelChunkExtractor,
};
use crate::application::{
    IngestConfig, IngestDocumentService, IngestionTrace, MaxConcurrency, ProviderConfig,
    ProviderMode, RunContext, RunErrorMetadata, RunMode, RunStatus, TraceableIngestError,
    TraceableIngestResult, utc_now_rfc3339,
};
use crate::domain::{
    EntityType, EpistemicStatus, GraphEdge, GraphNode, KnowledgeGraph, RelationshipType,
};
use crate::ports::{DocumentSource, GraphArtifactSink};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureConfig {
    #[serde(default = "default_tokenizer_name")]
    tokenizer_name: String,
    #[serde(default = "default_max_chunk_tokens")]
    max_chunk_tokens: usize,
    #[serde(default = "default_max_concurrency")]
    max_concurrency: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedExtraction {
    entities: Vec<ExpectedEntityMention>,
    relationships: Vec<ExpectedRelationshipMention>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedEntityMention {
    name: String,
    entity_type: EntityType,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedRelationshipMention {
    source: String,
    target: String,
    relationship_type: RelationshipType,
    description: String,
    evidence: Vec<ExpectedEvidence>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedGraph {
    nodes: Vec<ExpectedNode>,
    edges: Vec<ExpectedEdge>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedNode {
    id: String,
    name: String,
    entity_type: EntityType,
    aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedEdge {
    id: String,
    source: String,
    target: String,
    relationship_type: RelationshipType,
    description: String,
    weight: usize,
    evidence: Vec<ExpectedEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(deny_unknown_fields)]
struct ExpectedEvidence {
    fact: String,
    citation_text: String,
    status: EpistemicStatus,
}

#[derive(Debug)]
struct GoldFixture {
    id: String,
    input_path: PathBuf,
    config: FixtureConfig,
    expected_extraction: ExpectedExtraction,
    expected_graph: ExpectedGraph,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ActualNode {
    name: String,
    entity_type: EntityType,
    aliases: Vec<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ActualEdge {
    source: String,
    target: String,
    relationship_type: RelationshipType,
    description: String,
    weight: usize,
    evidence: Vec<ExpectedEvidence>,
}

struct EvalConfig {
    provider: ProviderConfig,
}

/// Evaluates all gold fixtures against a real configured provider.
///
/// # Errors
///
/// Returns an error when required evaluation environment variables are missing,
/// a fixture cannot be loaded, the provider client cannot be configured, or any
/// scenario's extraction/consolidation output differs from its gold expectation.
pub fn evaluate_gold_fixtures_from_env() -> Result<(), String> {
    let config = EvalConfig::from_env()?;
    let fixtures = load_gold_fixtures()?;

    let mut failures = Vec::new();

    for fixture in fixtures {
        if let Err(err) = evaluate_fixture(&fixture, &config) {
            failures.push(err);
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
}

fn evaluate_fixture(fixture: &GoldFixture, config: &EvalConfig) -> Result<(), String> {
    let run_context = fixture_run_context(fixture, config);
    log_fixture_started(&run_context, fixture);

    let document = load_fixture_document(fixture, &run_context)?;
    let client = configure_fixture_client(config, fixture, &run_context, &document)?;
    let extractor = build_fixture_extractor(fixture, client, &run_context, &document)?;
    let service = IngestDocumentService::new(extractor);
    let ingest_result = match execute_fixture_ingestion(fixture, &run_context, &service, &document)
    {
        Ok(result) => result,
        Err(err) => {
            return fixture_extraction_failure(fixture, &run_context, &document, err);
        }
    };
    let sections = collect_fixture_diffs(fixture, &ingest_result);

    if sections.is_empty() {
        Ok(())
    } else {
        fixture_mismatch_failure(fixture, &run_context, &document, &ingest_result, &sections)
    }
}

fn fixture_run_context(fixture: &GoldFixture, config: &EvalConfig) -> RunContext {
    RunContext::new(
        RunMode::GoldEval,
        fixture.input_path.clone(),
        None::<PathBuf>,
        &config.provider,
        &IngestConfig {
            tokenizer_name: fixture.config.tokenizer_name.clone(),
            max_chunk_tokens: fixture.config.max_chunk_tokens,
        },
        MaxConcurrency(fixture.config.max_concurrency),
    )
}

fn log_fixture_started(run_context: &RunContext, fixture: &GoldFixture) {
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        input_path = %fixture.input_path.display(),
        provider_mode = %run_context.provider.mode,
        tokenizer_name = %run_context.tokenizer_name,
        max_chunk_tokens = run_context.max_chunk_tokens,
        max_concurrency = run_context.max_concurrency,
        "run started"
    );
}

fn load_fixture_document(
    fixture: &GoldFixture,
    run_context: &RunContext,
) -> Result<crate::domain::Document, String> {
    let document = crate::adapters::FileDocumentSource
        .read_document(&fixture.input_path)
        .map_err(|err| {
            fixture_failure(
                run_context,
                &fixture.id,
                None,
                "read-input",
                format!("fixture {} failed to read input: {err}", fixture.id),
            )
        })?;
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        document_id = %document.id.0,
        document_bytes = document.text.0.len(),
        "document loaded"
    );
    Ok(document)
}

fn configure_fixture_client(
    config: &EvalConfig,
    fixture: &GoldFixture,
    run_context: &RunContext,
    document: &crate::domain::Document,
) -> Result<ConfiguredSchemaLlmClient, String> {
    ConfiguredSchemaLlmClient::from_config(&config.provider).map_err(|err| {
        fixture_failure(
            run_context,
            &fixture.id,
            Some(document.id.0.as_str()),
            "configure-provider-client",
            format!(
                "fixture {} failed to configure real provider client: {err}",
                fixture.id
            ),
        )
    })
}

fn build_fixture_extractor(
    fixture: &GoldFixture,
    client: ConfiguredSchemaLlmClient,
    run_context: &RunContext,
    document: &crate::domain::Document,
) -> Result<ParallelChunkExtractor<ConfiguredSchemaLlmClient>, String> {
    ParallelChunkExtractor::new(
        IngestConfig {
            tokenizer_name: fixture.config.tokenizer_name.clone(),
            max_chunk_tokens: fixture.config.max_chunk_tokens,
        },
        MaxConcurrency(fixture.config.max_concurrency),
        client,
        &HubTokenizerSource,
    )
    .map_err(|err| {
        fixture_failure(
            run_context,
            &fixture.id,
            Some(document.id.0.as_str()),
            "build-extractor",
            format!("fixture {} failed to build extractor: {err}", fixture.id),
        )
    })
}

fn execute_fixture_ingestion(
    _fixture: &GoldFixture,
    run_context: &RunContext,
    service: &IngestDocumentService<ParallelChunkExtractor<ConfiguredSchemaLlmClient>>,
    document: &crate::domain::Document,
) -> Result<TraceableIngestResult, TraceableIngestError> {
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        document_id = %document.id.0,
        "ingestion started"
    );
    let ingest_result = service.execute_with_trace(document)?;
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        document_id = %document.id.0,
        chunks = ingest_result.trace.chunks.len(),
        extracted_entities = ingest_result
            .trace
            .extracted_mentions
            .iter()
            .map(|chunk| chunk.entities.len())
            .sum::<usize>(),
        extracted_relationships = ingest_result
            .trace
            .extracted_mentions
            .iter()
            .map(|chunk| chunk.relationships.len())
            .sum::<usize>(),
        final_nodes = ingest_result.graph.nodes.len(),
        final_edges = ingest_result.graph.edges.len(),
        "ingestion completed"
    );
    Ok(ingest_result)
}

fn collect_fixture_diffs(
    fixture: &GoldFixture,
    ingest_result: &TraceableIngestResult,
) -> Vec<String> {
    let normalized_extraction = normalize_actual_extraction(&ingest_result.trace);
    let extraction_diff = diff_extraction(&fixture.expected_extraction, &normalized_extraction);
    let normalized_graph = normalize_actual_graph(&ingest_result.graph);
    let graph_diff = diff_graph(&fixture.expected_graph, &normalized_graph);

    let mut sections = Vec::new();
    if let Some(diff) = extraction_diff {
        sections.push(format!("Extraction correctness:\n{diff}"));
    }
    if let Some(diff) = graph_diff {
        sections.push(format!("Graph consolidation / end-to-end:\n{diff}"));
    }

    sections
}

fn fixture_mismatch_failure(
    fixture: &GoldFixture,
    run_context: &RunContext,
    document: &crate::domain::Document,
    ingest_result: &TraceableIngestResult,
    sections: &[String],
) -> Result<(), String> {
    let details = sections.join("\n");
    let head = format!("fixture {} did not match gold expectations", fixture.id);
    let context = FixtureFailureContext {
        fixture,
        run_context,
        document,
        graph: Some(&ingest_result.graph),
        trace: &ingest_result.trace,
    };
    fixture_failure_message(
        &context,
        &head,
        &details,
        RunErrorMetadata::new("gold-eval-mismatch", details.clone()),
    )
}

fn fixture_extraction_failure(
    fixture: &GoldFixture,
    run_context: &RunContext,
    document: &crate::domain::Document,
    err: TraceableIngestError,
) -> Result<(), String> {
    let TraceableIngestError { error, trace } = err;
    let head = format!("fixture {} failed during extraction: {}", fixture.id, error);
    let context = FixtureFailureContext {
        fixture,
        run_context,
        document,
        graph: None,
        trace: &trace,
    };
    fixture_failure_message(
        &context,
        &head,
        "",
        RunErrorMetadata::new(error.metadata_category(), error.to_string()),
    )
}

struct FixtureFailureContext<'a> {
    fixture: &'a GoldFixture,
    run_context: &'a RunContext,
    document: &'a crate::domain::Document,
    graph: Option<&'a KnowledgeGraph>,
    trace: &'a IngestionTrace,
}

fn fixture_failure_message(
    context: &FixtureFailureContext<'_>,
    head: &str,
    details: &str,
    error: RunErrorMetadata,
) -> Result<(), String> {
    let output_dir = temp_eval_debug_dir(&context.fixture.id);
    let metadata = context
        .run_context
        .with_output_dir(output_dir.clone())
        .finish_with_trace(
            Some(&context.document.id),
            Some(context.trace),
            utc_now_rfc3339(),
            RunStatus::Failure,
            Some(error),
        );
    let debug_output =
        persist_failure_artifacts(&output_dir, context.graph, context.trace, &metadata).ok();
    let mut message = head.to_owned();
    if let Some(path) = debug_output {
        let _ = write!(message, "\nDebug artifacts: {}", path.display());
    }
    if !details.is_empty() {
        message.push('\n');
        message.push_str(details);
    }
    debug!(
        run_id = %metadata.run_id,
        mode = %metadata.mode.label(),
        document_id = %context.document.id.0,
        error_category = %metadata.error.as_ref().map_or("<none>", |error| error.category.as_str()),
        error = %message,
        "run failed"
    );
    Err(message)
}

fn diff_extraction(expected: &ExpectedExtraction, actual: &NormalizedExtraction) -> Option<String> {
    let expected_entities = count_expected_entities(&expected.entities);
    let expected_relationships = count_expected_relationships(&expected.relationships);

    let mut sections = Vec::new();
    if let Some(diff) = diff_multiset("entities", &expected_entities, &actual.entities) {
        sections.push(diff);
    }
    if let Some(diff) = diff_multiset(
        "relationships",
        &expected_relationships,
        &actual.relationships,
    ) {
        sections.push(diff);
    }

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n"))
    }
}

fn diff_graph(expected: &ExpectedGraph, actual: &NormalizedGraph) -> Option<String> {
    let expected_nodes = expected
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.clone(),
                ActualNode {
                    name: node.name.clone(),
                    entity_type: node.entity_type.clone(),
                    aliases: sorted_unique(node.aliases.clone()),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let expected_edges = expected
        .edges
        .iter()
        .map(|edge| {
            (
                edge.id.clone(),
                ActualEdge {
                    source: edge.source.clone(),
                    target: edge.target.clone(),
                    relationship_type: edge.relationship_type.clone(),
                    description: edge.description.clone(),
                    weight: edge.weight,
                    evidence: sorted_unique(edge.evidence.clone()),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut sections = Vec::new();
    if let Some(diff) = diff_map("nodes", &expected_nodes, &actual.nodes) {
        sections.push(diff);
    }
    if let Some(diff) = diff_map("edges", &expected_edges, &actual.edges) {
        sections.push(diff);
    }

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n"))
    }
}

fn diff_multiset(
    label: &str,
    expected: &BTreeMap<String, usize>,
    actual: &BTreeMap<String, usize>,
) -> Option<String> {
    let missing = expected
        .iter()
        .filter_map(|(key, count)| {
            let actual_count = actual.get(key).copied().unwrap_or_default();
            (actual_count < *count).then(|| format!("{key} x{}", count - actual_count))
        })
        .collect::<Vec<_>>();
    let unexpected = actual
        .iter()
        .filter_map(|(key, count)| {
            let expected_count = expected.get(key).copied().unwrap_or_default();
            (expected_count < *count).then(|| format!("{key} x{}", count - expected_count))
        })
        .collect::<Vec<_>>();

    if missing.is_empty() && unexpected.is_empty() {
        return None;
    }

    let mut diff = String::new();
    let _ = writeln!(diff, "{label}:");
    if !missing.is_empty() {
        let _ = writeln!(diff, "  missing:");
        for item in missing {
            let _ = writeln!(diff, "    - {item}");
        }
    }
    if !unexpected.is_empty() {
        let _ = writeln!(diff, "  unexpected:");
        for item in unexpected {
            let _ = writeln!(diff, "    - {item}");
        }
    }

    Some(diff.trim_end().to_owned())
}

fn diff_map<T>(
    label: &str,
    expected: &BTreeMap<String, T>,
    actual: &BTreeMap<String, T>,
) -> Option<String>
where
    T: std::fmt::Debug + PartialEq,
{
    let mut missing = Vec::new();
    let mut unexpected = Vec::new();
    let mut mismatched = Vec::new();

    for (id, expected_value) in expected {
        match actual.get(id) {
            Some(actual_value) if actual_value != expected_value => {
                mismatched.push(format!(
                    "{id}\n      expected: {expected_value:?}\n      actual:   {actual_value:?}"
                ));
            }
            Some(_) => {}
            None => missing.push(id.clone()),
        }
    }

    for id in actual.keys() {
        if !expected.contains_key(id) {
            unexpected.push(id.clone());
        }
    }

    if missing.is_empty() && unexpected.is_empty() && mismatched.is_empty() {
        return None;
    }

    let mut diff = String::new();
    let _ = writeln!(diff, "{label}:");
    if !missing.is_empty() {
        let _ = writeln!(diff, "  missing:");
        for id in missing {
            let _ = writeln!(diff, "    - {id}");
        }
    }
    if !unexpected.is_empty() {
        let _ = writeln!(diff, "  unexpected:");
        for id in unexpected {
            let _ = writeln!(diff, "    - {id}");
        }
    }
    if !mismatched.is_empty() {
        let _ = writeln!(diff, "  mismatched:");
        for item in mismatched {
            let _ = writeln!(diff, "    - {item}");
        }
    }

    Some(diff.trim_end().to_owned())
}

struct NormalizedExtraction {
    entities: BTreeMap<String, usize>,
    relationships: BTreeMap<String, usize>,
}

struct NormalizedGraph {
    nodes: BTreeMap<String, ActualNode>,
    edges: BTreeMap<String, ActualEdge>,
}

fn normalize_actual_extraction(trace: &IngestionTrace) -> NormalizedExtraction {
    let mut entity_counts = BTreeMap::new();
    for chunk in &trace.extracted_mentions {
        for entity in &chunk.entities {
            let key = extraction_entity_key(
                entity.name.as_str(),
                entity.entity_type.as_str(),
                entity.description.as_str(),
            );
            *entity_counts.entry(key).or_insert(0) += 1;
        }
    }

    let mut relationship_counts = BTreeMap::new();
    for chunk in &trace.extracted_mentions {
        for relationship in &chunk.relationships {
            let evidence = relationship
                .evidence
                .iter()
                .map(|item| {
                    extraction_evidence_key(
                        item.fact.as_str(),
                        item.citation_text.as_str(),
                        item.status.as_str(),
                    )
                })
                .collect::<Vec<_>>();
            let evidence = sorted_unique(evidence);
            let key = extraction_relationship_key(
                relationship.source.as_str(),
                relationship.target.as_str(),
                relationship.relationship_type.as_str(),
                relationship.description.as_str(),
                &evidence,
            );
            *relationship_counts.entry(key).or_insert(0) += 1;
        }
    }

    NormalizedExtraction {
        entities: entity_counts,
        relationships: relationship_counts,
    }
}

fn normalize_actual_graph(graph: &KnowledgeGraph) -> NormalizedGraph {
    let nodes = graph
        .nodes
        .iter()
        .map(|node| (node.id.0.clone(), normalize_node(node)))
        .collect::<BTreeMap<_, _>>();
    let edges = graph
        .edges
        .iter()
        .map(|edge| (edge.id.0.clone(), normalize_edge(edge)))
        .collect::<BTreeMap<_, _>>();

    NormalizedGraph { nodes, edges }
}

fn normalize_node(node: &GraphNode) -> ActualNode {
    ActualNode {
        name: node.name.0.clone(),
        entity_type: node.entity_type.clone(),
        aliases: sorted_unique(node.aliases.iter().map(|alias| alias.0.clone()).collect()),
    }
}

fn normalize_edge(edge: &GraphEdge) -> ActualEdge {
    ActualEdge {
        source: edge.source.0.clone(),
        target: edge.target.0.clone(),
        relationship_type: edge.edge_type.clone(),
        description: edge.description.0.clone(),
        weight: usize::from(edge.weight.0),
        evidence: sorted_unique(
            edge.evidence
                .iter()
                .map(|claim| ExpectedEvidence {
                    fact: claim.fact.0.clone(),
                    citation_text: claim.citation_text.clone(),
                    status: claim.status.clone(),
                })
                .collect(),
        ),
    }
}

fn count_expected_entities(entities: &[ExpectedEntityMention]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entity in entities {
        let entity_type = format!("{:?}", entity.entity_type);
        let key = extraction_entity_key(
            entity.name.as_str(),
            entity_type.as_str(),
            entity.description.as_str(),
        );
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn count_expected_relationships(
    relationships: &[ExpectedRelationshipMention],
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for relationship in relationships {
        let relationship_type = format!("{:?}", relationship.relationship_type);
        let evidence = relationship
            .evidence
            .iter()
            .map(|item| {
                let status = format!("{:?}", item.status);
                extraction_evidence_key(
                    item.fact.as_str(),
                    item.citation_text.as_str(),
                    status.as_str(),
                )
            })
            .collect::<Vec<_>>();
        let evidence = sorted_unique(evidence);
        let key = extraction_relationship_key(
            relationship.source.as_str(),
            relationship.target.as_str(),
            relationship_type.as_str(),
            relationship.description.as_str(),
            &evidence,
        );
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn extraction_entity_key(name: &str, entity_type: &str, description: &str) -> String {
    format!("name={name:?}|type={entity_type}|description={description:?}")
}

fn extraction_relationship_key(
    source: &str,
    target: &str,
    relationship_type: &str,
    description: &str,
    evidence: &[String],
) -> String {
    format!(
        "source={source:?}|target={target:?}|type={relationship_type}|description={description:?}|evidence={evidence:?}"
    )
}

fn extraction_evidence_key(fact: &str, citation_text: &str, status: &str) -> String {
    format!("fact={fact:?}|citation_text={citation_text:?}|status={status}")
}

fn persist_failure_artifacts(
    output_dir: &Path,
    graph: Option<&KnowledgeGraph>,
    trace: &IngestionTrace,
    metadata: &crate::application::RunMetadata,
) -> Result<PathBuf, String> {
    if let Some(graph) = graph {
        FileGraphArtifactSink
            .write_graph(output_dir, graph)
            .map_err(|err| format!("failed to write eval graph artifacts: {err}"))?;
    }
    FileGraphArtifactSink
        .write_debug_artifacts(output_dir, trace, metadata)
        .map_err(|err| format!("failed to write eval debug artifacts: {err}"))?;
    Ok(output_dir.to_path_buf())
}

fn fixture_failure(
    run_context: &RunContext,
    fixture_id: &str,
    document_id: Option<&str>,
    category: &str,
    message: String,
) -> String {
    debug!(
        run_id = %run_context.run_id,
        mode = %run_context.mode.label(),
        fixture_id = %fixture_id,
        document_id = %document_id.unwrap_or("<none>"),
        error_category = %category,
        error = %message,
        "run failed"
    );
    message
}

fn temp_eval_debug_dir(fixture_id: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("kg_tdd_eval_debug_{fixture_id}_{unique}"))
}

fn sorted_unique<T>(items: Vec<T>) -> Vec<T>
where
    T: Clone + Ord,
{
    items
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

impl EvalConfig {
    fn from_env() -> Result<Self, String> {
        let base_url = read_required_env("KG_EVAL_PROVIDER_BASE_URL")?;
        let model = read_required_env("KG_EVAL_PROVIDER_MODEL")?;
        let api_key = std::env::var("KG_EVAL_PROVIDER_API_KEY").ok();

        let _ = api_key;

        Ok(Self {
            provider: ProviderConfig {
                mode: ProviderMode::OpenAiCompatible,
                base_url: Some(base_url),
                model: Some(model),
            },
        })
    }
}

fn read_required_env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .map_err(|_| format!("missing required evaluation environment variable: {name}"))
}

fn gold_fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("gold")
}

fn default_tokenizer_name() -> String {
    String::from("bert-base-cased")
}

fn default_max_chunk_tokens() -> usize {
    128
}

fn default_max_concurrency() -> u8 {
    4
}

fn load_gold_fixtures() -> Result<Vec<GoldFixture>, String> {
    load_gold_fixtures_from_root(&gold_fixtures_root())
}

fn load_gold_fixtures_from_root(root: &Path) -> Result<Vec<GoldFixture>, String> {
    let mut fixtures = Vec::new();

    let entries = fs::read_dir(root).map_err(|err| {
        format!(
            "failed to read gold fixtures root {}: {err}",
            root.display()
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read gold fixture entry: {err}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let id = entry.file_name().to_string_lossy().into_owned();
        let input_path = path.join("input.txt");
        let expected_path = path.join("expected.json");
        let expected_extraction_path = path.join("expected_extraction.json");
        let config_path = path.join("config.json");

        if !input_path.is_file() {
            return Err(format!(
                "gold fixture {id} is missing required file {}",
                input_path.display()
            ));
        }
        if !expected_path.is_file() {
            return Err(format!(
                "gold fixture {id} is missing required file {}",
                expected_path.display()
            ));
        }
        if !expected_extraction_path.is_file() {
            return Err(format!(
                "gold fixture {id} is missing required file {}",
                expected_extraction_path.display()
            ));
        }

        let expected_graph = read_json::<ExpectedGraph>(&expected_path, &id, "expected graph")?;
        let expected_extraction =
            read_json::<ExpectedExtraction>(&expected_extraction_path, &id, "expected extraction")?;
        let config = if config_path.is_file() {
            read_json::<FixtureConfig>(&config_path, &id, "fixture config")?
        } else {
            FixtureConfig {
                tokenizer_name: default_tokenizer_name(),
                max_chunk_tokens: default_max_chunk_tokens(),
                max_concurrency: default_max_concurrency(),
            }
        };

        fixtures.push(GoldFixture {
            id,
            input_path,
            config,
            expected_extraction,
            expected_graph,
        });
    }

    fixtures.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(fixtures)
}

fn read_json<T>(path: &Path, id: &str, label: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let raw = fs::read_to_string(path).map_err(|err| {
        format!(
            "failed to read {label} for fixture {id} from {}: {err}",
            path.display()
        )
    })?;
    serde_json::from_str(&raw).map_err(|err| {
        format!(
            "failed to parse {label} for fixture {id} from {}: {err}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        EvalConfig, ExpectedExtraction, ExpectedGraph, FixtureConfig, GoldFixture,
        default_max_chunk_tokens, default_max_concurrency, default_tokenizer_name, diff_multiset,
        fixture_extraction_failure, fixture_run_context, gold_fixtures_root, load_gold_fixtures,
        load_gold_fixtures_from_root, persist_failure_artifacts,
    };
    use crate::application::{
        ChunkExtractionTrace, ChunkTrace, EntityMentionTrace, IngestionTrace, MaxConcurrency,
        ProviderConfig, ProviderResponseKind, ProviderResponseTrace, RunContext, RunErrorMetadata,
        RunMode, RunStatus, TraceableIngestError, TraceableIngestResult, utc_now_rfc3339,
    };
    use crate::domain::{Document, DocumentId, KnowledgeGraph, NonEmptyString};

    #[test]
    fn gold_fixtures_have_required_files() {
        let fixtures = load_gold_fixtures().expect("gold fixtures load");

        assert_eq!(fixtures.len(), 4);
        for fixture in fixtures {
            assert!(fixture.input_path.is_file());
            let fixture_dir = fixture
                .input_path
                .parent()
                .expect("fixture directory should exist");
            assert!(fixture_dir.join("expected.json").is_file());
            assert!(fixture_dir.join("expected_extraction.json").is_file());
        }
    }

    #[test]
    fn gold_expected_graphs_and_extractions_deserialize_into_fixture_schema() {
        let fixtures = load_gold_fixtures().expect("gold fixtures load");

        for fixture in fixtures {
            assert!(
                !fixture.expected_graph.nodes.is_empty(),
                "fixture {} should define at least one expected node",
                fixture.id
            );
            assert!(
                fixture.config.max_chunk_tokens > 0,
                "fixture {} has invalid max_chunk_tokens",
                fixture.id
            );
            assert!(
                fixture.config.max_concurrency > 0,
                "fixture {} has invalid max_concurrency",
                fixture.id
            );
            for node in &fixture.expected_graph.nodes {
                assert!(
                    !node.id.is_empty(),
                    "fixture {} has empty node id",
                    fixture.id
                );
                assert!(
                    !node.name.is_empty(),
                    "fixture {} has empty node name",
                    fixture.id
                );
            }
            for entity in &fixture.expected_extraction.entities {
                assert!(
                    !entity.name.is_empty(),
                    "fixture {} has empty extraction entity name",
                    fixture.id
                );
                assert!(
                    !entity.description.is_empty(),
                    "fixture {} has empty extraction entity description",
                    fixture.id
                );
            }
        }
    }

    #[test]
    fn gold_fixture_ids_are_unique_and_include_seed() {
        let fixtures = load_gold_fixtures().expect("gold fixtures load");
        let ids = fixtures
            .iter()
            .map(|fixture| fixture.id.clone())
            .collect::<Vec<_>>();
        let unique_ids = ids.iter().cloned().collect::<HashSet<_>>();

        assert_eq!(ids.len(), unique_ids.len());
        assert_eq!(
            ids,
            vec![
                String::from("alias-merge"),
                String::from("duplicate-evidence-merge"),
                String::from("relationship-type-collision"),
                String::from("seed"),
            ]
        );
        assert!(gold_fixtures_root().join("seed").is_dir());
    }

    #[test]
    fn fixture_loader_rejects_missing_expected_extraction_file() {
        let root = temp_dir("missing_extraction");
        let fixture_dir = root.join("sample");
        fs::create_dir_all(&fixture_dir).expect("fixture dir");
        fs::write(fixture_dir.join("input.txt"), "hello").expect("input");
        fs::write(
            fixture_dir.join("expected.json"),
            r#"{"nodes":[{"id":"node:lifeform:apple","name":"apple","entity_type":"Lifeform","aliases":[]}],"edges":[]}"#,
        )
        .expect("expected graph");

        let err = load_gold_fixtures_from_root(&root).expect_err("missing expected extraction");

        assert!(err.contains("expected_extraction.json"));
    }

    #[test]
    fn diff_output_is_human_readable() {
        let mut expected = std::collections::BTreeMap::new();
        expected.insert(String::from("alpha"), 2);
        let mut actual = std::collections::BTreeMap::new();
        actual.insert(String::from("alpha"), 1);
        actual.insert(String::from("beta"), 1);

        let diff = diff_multiset("entities", &expected, &actual).expect("diff");

        assert!(diff.contains("entities:"));
        assert!(diff.contains("missing:"));
        assert!(diff.contains("unexpected:"));
        assert!(diff.contains("alpha x1"));
        assert!(diff.contains("beta x1"));
    }

    #[test]
    fn gold_failure_artifacts_include_run_metadata() {
        let result = TraceableIngestResult {
            graph: KnowledgeGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            },
            trace: IngestionTrace::default(),
        };
        let context = RunContext::new(
            RunMode::GoldEval,
            PathBuf::from("tests/fixtures/gold/seed/input.txt"),
            None::<PathBuf>,
            &ProviderConfig::default(),
            &crate::application::IngestConfig::default(),
            MaxConcurrency(1),
        );
        let metadata = context.finish(
            None,
            Some(&result),
            utc_now_rfc3339(),
            RunStatus::Failure,
            Some(RunErrorMetadata::new(
                "gold-eval-mismatch",
                "fixture seed did not match gold expectations",
            )),
        );

        let output_dir = temp_dir("gold_failure_artifacts");
        let output_dir =
            persist_failure_artifacts(&output_dir, Some(&result.graph), &result.trace, &metadata)
                .expect("artifacts");
        let metadata_path = output_dir.join("debug").join("run-metadata.json");

        assert!(metadata_path.is_file());
        assert!(
            std::fs::read_to_string(metadata_path)
                .expect("metadata")
                .contains("\"status\": \"failure\"")
        );
    }

    #[test]
    fn extraction_failures_write_debug_artifacts_and_report_path() {
        let fixture = GoldFixture {
            id: String::from("seed"),
            input_path: PathBuf::from("tests/fixtures/gold/seed/input.txt"),
            config: FixtureConfig {
                tokenizer_name: default_tokenizer_name(),
                max_chunk_tokens: default_max_chunk_tokens(),
                max_concurrency: default_max_concurrency(),
            },
            expected_extraction: ExpectedExtraction {
                entities: Vec::new(),
                relationships: Vec::new(),
            },
            expected_graph: ExpectedGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            },
        };
        let run_context = fixture_run_context(
            &fixture,
            &EvalConfig {
                provider: ProviderConfig::default(),
            },
        );
        let document = Document {
            id: DocumentId(String::from("doc-1")),
            text: NonEmptyString(String::from("ignored")),
        };
        let trace = IngestionTrace {
            chunks: vec![ChunkTrace {
                index: 0,
                document_id: document.id.0.clone(),
                text: String::from("Alice met Bob"),
                token_count: 4,
            }],
            provider_responses: vec![ProviderResponseTrace {
                chunk_index: 0,
                kind: ProviderResponseKind::EntityExtraction,
                raw_response: String::from(
                    "{\"entities\":[{\"name\":\"Alice\",\"entity_type\":\"Person\"}]}",
                ),
            }],
            extracted_mentions: vec![ChunkExtractionTrace {
                chunk_index: 0,
                entities: vec![EntityMentionTrace {
                    name: String::from("Alice"),
                    entity_type: String::from("Person"),
                    description: String::from("person"),
                    source_document_id: document.id.0.clone(),
                    source_text: String::from("Alice met Bob"),
                    source_token_count: 4,
                }],
                relationships: Vec::new(),
            }],
        };
        let err = TraceableIngestError::new(
            crate::application::AppError::provider_response(
                "assistant content does not match schema",
            ),
            trace,
        );

        let message = fixture_extraction_failure(&fixture, &run_context, &document, err)
            .expect_err("extraction failure should be reported");

        assert!(message.contains("fixture seed failed during extraction"));
        assert!(message.contains("Debug artifacts:"));

        let artifact_dir = message
            .lines()
            .find_map(|line: &str| line.strip_prefix("Debug artifacts: ").map(PathBuf::from))
            .expect("artifact path");
        let metadata_path = artifact_dir.join("debug").join("run-metadata.json");
        let chunks_path = artifact_dir.join("debug").join("chunk-list.json");
        let responses_path = artifact_dir
            .join("debug")
            .join("raw-provider-responses.json");
        let mentions_path = artifact_dir.join("debug").join("extracted-mentions.json");

        assert!(metadata_path.is_file());
        assert!(chunks_path.is_file());
        assert!(responses_path.is_file());
        assert!(mentions_path.is_file());
        assert!(
            std::fs::read_to_string(metadata_path)
                .expect("metadata")
                .contains("\"chunk_count\": 1")
        );
    }

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("kg_tdd_eval_{label}_{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
