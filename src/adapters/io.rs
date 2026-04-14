use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use tracing::debug;

use crate::application::{AppError, IngestionTrace, RunMetadata};
use crate::domain::{Document, DocumentId, KnowledgeGraph, NonEmptyString};
use crate::ports::{DocumentSource, GraphArtifactSink};

use super::CytoscapeJsonProjector;

pub(crate) struct FileDocumentSource;

impl DocumentSource for FileDocumentSource {
    fn read_document(&self, input_path: &Path) -> Result<Document, AppError> {
        debug!("reading document from {}", input_path.display());
        if !input_path.is_file() {
            return Err(AppError::invalid_input_path(input_path));
        }

        let raw_text =
            fs::read_to_string(input_path).map_err(|_| AppError::read_input(input_path))?;
        if raw_text.trim().is_empty() {
            return Err(AppError::empty_input(input_path));
        }

        debug!(
            "read document from {} ({} bytes)",
            input_path.display(),
            raw_text.len()
        );

        Ok(Document {
            id: DocumentId(document_id_from_path(input_path)),
            text: NonEmptyString(raw_text),
        })
    }
}

pub(crate) struct FileGraphArtifactSink;

impl GraphArtifactSink for FileGraphArtifactSink {
    fn write_graph(&self, output_dir: &Path, graph: &KnowledgeGraph) -> Result<(), AppError> {
        debug!(
            "writing graph artifact bundle to {} (nodes={}, edges={})",
            output_dir.display(),
            graph.nodes.len(),
            graph.edges.len()
        );
        fs::create_dir_all(output_dir).map_err(|_| AppError::create_output_dir(output_dir))?;

        let graph_path = output_dir.join("graph.json");
        let graph_json = CytoscapeJsonProjector::project(graph)?;
        fs::write(&graph_path, graph_json).map_err(|_| AppError::write_output(&graph_path))?;

        copy_viewer_asset(output_dir, "index.html")?;
        copy_viewer_asset(output_dir, "cytoscape.min.js")?;

        Ok(())
    }

    fn write_debug_artifacts(
        &self,
        output_dir: &Path,
        trace: &IngestionTrace,
        metadata: &RunMetadata,
    ) -> Result<(), AppError> {
        let debug_dir = output_dir.join("debug");
        fs::create_dir_all(&debug_dir).map_err(|_| AppError::create_output_dir(&debug_dir))?;

        write_json_file(&debug_dir.join("run-metadata.json"), metadata)?;
        write_json_file(&debug_dir.join("chunk-list.json"), &trace.chunks)?;
        write_json_file(
            &debug_dir.join("raw-provider-responses.json"),
            &trace.provider_responses,
        )?;
        write_json_file(
            &debug_dir.join("extracted-mentions.json"),
            &trace.extracted_mentions,
        )?;

        Ok(())
    }
}

fn write_json_file<T>(path: &Path, value: &T) -> Result<(), AppError>
where
    T: serde::Serialize,
{
    let json = serde_json::to_string_pretty(value).map_err(|_| AppError::write_output(path))?;
    fs::write(path, json).map_err(|_| AppError::write_output(path))
}

fn copy_viewer_asset(output_dir: &Path, file_name: &str) -> Result<(), AppError> {
    let source_path = viewer_asset_path(file_name);
    let target_path = output_dir.join(file_name);
    debug!(
        "copying viewer asset {} to {}",
        source_path.display(),
        target_path.display()
    );
    fs::copy(&source_path, &target_path).map_err(|_| AppError::write_output(target_path))?;
    Ok(())
}

fn viewer_asset_path(file_name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("viewer")
        .join(file_name)
}

fn document_id_from_path(path: &Path) -> String {
    let display_name = path.file_stem().or_else(|| path.file_name()).map_or_else(
        || String::from("document"),
        |value| value.to_string_lossy().into_owned(),
    );

    let mut slug = slugify(&display_name);
    if slug.is_empty() {
        slug = String::from("document");
    }

    let mut hasher = DefaultHasher::new();
    normalize_path(path).hash(&mut hasher);

    format!("{slug}-{:016x}", hasher.finish())
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    slug.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        FileDocumentSource, FileGraphArtifactSink, document_id_from_path, viewer_asset_path,
    };
    use crate::application::{
        ChunkExtractionTrace, ChunkTrace, EntityMentionTrace, EvidenceTrace, IngestConfig,
        IngestionTrace, MaxConcurrency, ProviderConfig, ProviderResponseKind,
        ProviderResponseTrace, RelationshipMentionTrace, RunContext, RunMode, RunStatus,
        utc_now_rfc3339,
    };
    use crate::domain::KnowledgeGraph;
    use crate::ports::{DocumentSource, GraphArtifactSink};

    #[test]
    fn reads_document_from_file_path() {
        let dir = temp_dir("read_document");
        let input_path = dir.join("Seed Document.txt");
        fs::write(&input_path, "Alpha beta").expect("write input");

        let document = FileDocumentSource
            .read_document(&input_path)
            .expect("document reads");

        assert!(document.id.0.starts_with("seed-document-"));
        assert_eq!(document.text.0, "Alpha beta");
    }

    #[test]
    fn rejects_empty_input_file() {
        let dir = temp_dir("empty_document");
        let input_path = dir.join("empty.txt");
        fs::write(&input_path, "   ").expect("write input");

        let err = FileDocumentSource
            .read_document(&input_path)
            .expect_err("empty file should fail");

        assert_eq!(
            err.to_string(),
            format!("input file is empty: {}", input_path.display())
        );
    }

    #[test]
    fn writes_graph_artifact_bundle_to_output_directory() {
        let dir = temp_dir("write_graph");
        let output_dir = dir.join("artifacts");

        FileGraphArtifactSink
            .write_graph(
                &output_dir,
                &KnowledgeGraph {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                },
            )
            .expect("write graph");

        let graph_path = output_dir.join("graph.json");
        let index_path = output_dir.join("index.html");
        let script_path = output_dir.join("cytoscape.min.js");
        assert!(graph_path.is_file());
        assert!(index_path.is_file());
        assert!(script_path.is_file());
        assert_eq!(
            fs::read_to_string(graph_path).expect("graph json"),
            r#"{"nodes":[],"edges":[]}"#
        );
        let index_html = fs::read_to_string(index_path).expect("index html");
        assert!(index_html.contains("./cytoscape.min.js"));
        assert!(index_html.contains("./graph.json"));
        assert!(!index_html.contains("http://"));
        assert!(!index_html.contains("https://"));
        assert_eq!(
            fs::read(script_path).expect("script bytes"),
            fs::read(viewer_asset_path("cytoscape.min.js")).expect("vendored script bytes")
        );
    }

    #[test]
    fn writes_debug_trace_artifacts_under_debug_directory() {
        let dir = temp_dir("write_debug");
        let output_dir = dir.join("artifacts");

        FileGraphArtifactSink
            .write_debug_artifacts(&output_dir, &sample_trace(), &sample_metadata())
            .expect("write debug artifacts");

        let debug_dir = output_dir.join("debug");
        let metadata_path = debug_dir.join("run-metadata.json");
        let chunks_path = debug_dir.join("chunk-list.json");
        let responses_path = debug_dir.join("raw-provider-responses.json");
        let mentions_path = debug_dir.join("extracted-mentions.json");

        assert!(metadata_path.is_file());
        assert!(chunks_path.is_file());
        assert!(responses_path.is_file());
        assert!(mentions_path.is_file());
        assert!(
            fs::read_to_string(metadata_path)
                .expect("metadata")
                .contains("\"run_id\"")
        );
        assert!(
            fs::read_to_string(chunks_path)
                .expect("chunks")
                .contains("\"doc-1\"")
        );
        assert!(
            fs::read_to_string(responses_path)
                .expect("responses")
                .contains("\"EntityExtraction\"")
        );
        assert!(
            fs::read_to_string(mentions_path)
                .expect("mentions")
                .contains("\"Alice\"")
        );
    }

    fn sample_metadata() -> crate::application::RunMetadata {
        let context = RunContext::new(
            RunMode::Cli,
            PathBuf::from("fixtures/input.txt"),
            Some(PathBuf::from("out")),
            &ProviderConfig::default(),
            &IngestConfig::default(),
            MaxConcurrency(1),
        );

        context.finish(None, None, utc_now_rfc3339(), RunStatus::Success, None)
    }

    #[test]
    fn document_ids_are_stable_for_the_same_path() {
        let path = PathBuf::from("/tmp/Seed Document.txt");

        let first = document_id_from_path(&path);
        let second = document_id_from_path(&path);

        assert_eq!(first, second);
    }

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("kg_tdd_{label}_{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn sample_trace() -> IngestionTrace {
        IngestionTrace {
            chunks: vec![ChunkTrace {
                index: 0,
                document_id: String::from("doc-1"),
                text: String::from("Alice met Bob"),
                token_count: 4,
            }],
            provider_responses: vec![ProviderResponseTrace {
                chunk_index: 0,
                kind: ProviderResponseKind::EntityExtraction,
                raw_response: String::from("{\"entities\":[{\"name\":\"Alice\"}]}"),
            }],
            extracted_mentions: vec![ChunkExtractionTrace {
                chunk_index: 0,
                entities: vec![EntityMentionTrace {
                    name: String::from("Alice"),
                    entity_type: String::from("Person"),
                    description: String::from("person"),
                    source_document_id: String::from("doc-1"),
                    source_text: String::from("<entity>Alice</entity> met <entity>Bob</entity>"),
                    source_token_count: 8,
                }],
                relationships: vec![RelationshipMentionTrace {
                    source: String::from("node:person:alice"),
                    target: String::from("node:person:bob"),
                    relationship_type: String::from("IsA"),
                    description: String::from("knows"),
                    evidence: vec![EvidenceTrace {
                        fact: String::from("Alice met Bob"),
                        citation_text: String::from("Alice met Bob"),
                        status: String::from("Probable"),
                        source_document_id: String::from("doc-1"),
                        source_text: String::from(
                            "<entity>Alice</entity> met <entity>Bob</entity>",
                        ),
                        source_token_count: 8,
                    }],
                }],
            }],
        }
    }
}
