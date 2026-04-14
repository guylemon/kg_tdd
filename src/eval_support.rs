use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::adapters::{ConfiguredSchemaLlmClient, HubTokenizerSource, ParallelChunkExtractor};
use crate::application::{IngestConfig, MaxConcurrency, ProviderConfig, ProviderMode};
use crate::domain::{
    EntityMention, EntityType, EpistemicStatus, GraphEdge, GraphNode, KnowledgeGraph,
    RelationshipMention, RelationshipType, consolidate_entities, consolidate_relationships,
};
use crate::ports::{ChunkExtractor, DocumentPartitioner, DocumentSource};

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
    let document = crate::adapters::FileDocumentSource
        .read_document(&fixture.input_path)
        .map_err(|err| format!("fixture {} failed to read input: {err}", fixture.id))?;

    let client = ConfiguredSchemaLlmClient::from_config(&config.provider).map_err(|err| {
        format!(
            "fixture {} failed to configure real provider client: {err}",
            fixture.id
        )
    })?;
    let extractor = ParallelChunkExtractor::new(
        IngestConfig {
            tokenizer_name: fixture.config.tokenizer_name.clone(),
            max_chunk_tokens: fixture.config.max_chunk_tokens,
        },
        MaxConcurrency(fixture.config.max_concurrency),
        client,
        &HubTokenizerSource,
    )
    .map_err(|err| format!("fixture {} failed to build extractor: {err}", fixture.id))?;

    let chunks = extractor
        .partition(&document)
        .map_err(|err| format!("fixture {} failed to partition input: {err}", fixture.id))?;

    let mut entities = Vec::new();
    let mut relationships = Vec::new();

    for chunk in chunks {
        let extraction = extractor
            .extract(chunk)
            .map_err(|err| format!("fixture {} failed during extraction: {err}", fixture.id))?;
        entities.extend(extraction.entities);
        relationships.extend(extraction.relationships);
    }

    let normalized_extraction = normalize_actual_extraction(&entities, &relationships);
    let extraction_diff = diff_extraction(&fixture.expected_extraction, &normalized_extraction);
    let graph = KnowledgeGraph {
        nodes: consolidate_entities(entities),
        edges: consolidate_relationships(relationships),
    };
    let normalized_graph = normalize_actual_graph(&graph);
    let graph_diff = diff_graph(&fixture.expected_graph, &normalized_graph);

    let mut sections = Vec::new();
    if let Some(diff) = extraction_diff {
        sections.push(format!("Extraction correctness:\n{diff}"));
    }
    if let Some(diff) = graph_diff {
        sections.push(format!("Graph consolidation / end-to-end:\n{diff}"));
    }

    if sections.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "fixture {} did not match gold expectations\n{}",
            fixture.id,
            sections.join("\n")
        ))
    }
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

fn normalize_actual_extraction(
    entities: &[EntityMention],
    relationships: &[RelationshipMention],
) -> NormalizedExtraction {
    let mut entity_counts = BTreeMap::new();
    for entity in entities {
        let key = format!(
            "name={:?}|type={:?}|description={:?}",
            entity.name.0, entity.entity_type, entity.description.0
        );
        *entity_counts.entry(key).or_insert(0) += 1;
    }

    let mut relationship_counts = BTreeMap::new();
    for relationship in relationships {
        let evidence = relationship
            .evidence
            .iter()
            .map(|claim| ExpectedEvidence {
                fact: claim.fact.0.clone(),
                citation_text: claim.citation_text.clone(),
                status: claim.status.clone(),
            })
            .collect::<Vec<_>>();
        let key = format!(
            "source={:?}|target={:?}|type={:?}|description={:?}|evidence={:?}",
            relationship.source.0,
            relationship.target.0,
            relationship.relationship_type,
            relationship.description.0,
            sorted_unique(evidence)
        );
        *relationship_counts.entry(key).or_insert(0) += 1;
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
        let key = format!(
            "name={:?}|type={:?}|description={:?}",
            entity.name, entity.entity_type, entity.description
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
        let key = format!(
            "source={:?}|target={:?}|type={:?}|description={:?}|evidence={:?}",
            relationship.source,
            relationship.target,
            relationship.relationship_type,
            relationship.description,
            sorted_unique(relationship.evidence.clone())
        );
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
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
        diff_multiset, gold_fixtures_root, load_gold_fixtures, load_gold_fixtures_from_root,
    };

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
