use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::{EntityType, EpistemicStatus, RelationshipType};

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

#[derive(Debug, Deserialize)]
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
    expected_path: PathBuf,
    expected: ExpectedGraph,
}

fn gold_fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("gold")
}

fn load_gold_fixtures() -> Result<Vec<GoldFixture>, String> {
    let mut fixtures = Vec::new();
    let root = gold_fixtures_root();

    let entries = fs::read_dir(&root).map_err(|err| {
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

        let expected_json = fs::read_to_string(&expected_path).map_err(|err| {
            format!(
                "failed to read expected graph for fixture {id} from {}: {err}",
                expected_path.display()
            )
        })?;
        let expected = serde_json::from_str::<ExpectedGraph>(&expected_json).map_err(|err| {
            format!(
                "failed to parse expected graph for fixture {id} from {}: {err}",
                expected_path.display()
            )
        })?;

        fixtures.push(GoldFixture {
            id,
            input_path,
            expected_path,
            expected,
        });
    }

    fixtures.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(fixtures)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{gold_fixtures_root, load_gold_fixtures};

    #[test]
    fn gold_fixtures_have_required_files() {
        let fixtures = load_gold_fixtures().expect("gold fixtures load");

        assert_eq!(fixtures.len(), 4);
        for fixture in fixtures {
            assert!(fixture.input_path.is_file());
            assert!(fixture.expected_path.is_file());
        }
    }

    #[test]
    fn gold_expected_graphs_deserialize_into_fixture_schema() {
        let fixtures = load_gold_fixtures().expect("gold fixtures load");

        for fixture in fixtures {
            assert!(
                !fixture.expected.nodes.is_empty(),
                "fixture {} should define at least one expected node",
                fixture.id
            );
            for node in &fixture.expected.nodes {
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
                assert!(
                    matches!(
                        node.entity_type,
                        crate::domain::EntityType::Concept
                            | crate::domain::EntityType::Lifeform
                            | crate::domain::EntityType::Organization
                    ),
                    "fixture {} uses an unexpected entity type for v1 gold fixtures",
                    fixture.id
                );
                let unique_aliases = node.aliases.iter().collect::<HashSet<_>>();
                assert_eq!(
                    node.aliases.len(),
                    unique_aliases.len(),
                    "fixture {} contains duplicate node aliases",
                    fixture.id
                );
            }

            for edge in &fixture.expected.edges {
                assert!(
                    !edge.id.is_empty(),
                    "fixture {} has empty edge id",
                    fixture.id
                );
                assert!(
                    !edge.source.is_empty(),
                    "fixture {} has empty edge source",
                    fixture.id
                );
                assert!(
                    !edge.target.is_empty(),
                    "fixture {} has empty edge target",
                    fixture.id
                );
                assert!(
                    !edge.description.is_empty(),
                    "fixture {} has empty edge description",
                    fixture.id
                );
                assert!(
                    edge.weight > 0,
                    "fixture {} has zero edge weight",
                    fixture.id
                );
                assert!(
                    matches!(
                        edge.relationship_type,
                        crate::domain::RelationshipType::GrowsOn
                            | crate::domain::RelationshipType::IsA
                    ),
                    "fixture {} uses an unexpected relationship type for v1 gold fixtures",
                    fixture.id
                );
                for evidence in &edge.evidence {
                    assert!(
                        !evidence.fact.is_empty(),
                        "fixture {} has empty evidence fact",
                        fixture.id
                    );
                    assert!(
                        !evidence.citation_text.is_empty(),
                        "fixture {} has empty evidence citation_text",
                        fixture.id
                    );
                    assert!(
                        matches!(evidence.status, crate::domain::EpistemicStatus::Probable),
                        "fixture {} uses an unexpected evidence status for v1 gold fixtures",
                        fixture.id
                    );
                }
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
}
