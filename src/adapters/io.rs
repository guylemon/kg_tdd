use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::application::AppError;
use crate::domain::{Document, DocumentId, KnowledgeGraph, NonEmptyString};
use crate::ports::{DocumentSource, GraphArtifactSink};

use super::CytoscapeJsonProjector;

pub(crate) struct FileDocumentSource;

impl DocumentSource for FileDocumentSource {
    fn read_document(&self, input_path: &Path) -> Result<Document, AppError> {
        if !input_path.is_file() {
            return Err(AppError::invalid_input_path(input_path));
        }

        let raw_text =
            fs::read_to_string(input_path).map_err(|_| AppError::read_input(input_path))?;
        if raw_text.trim().is_empty() {
            return Err(AppError::empty_input(input_path));
        }

        Ok(Document {
            id: DocumentId(document_id_from_path(input_path)),
            text: NonEmptyString(raw_text),
        })
    }
}

pub(crate) struct FileGraphArtifactSink;

impl GraphArtifactSink for FileGraphArtifactSink {
    fn write_graph(&self, output_dir: &Path, graph: &KnowledgeGraph) -> Result<(), AppError> {
        fs::create_dir_all(output_dir).map_err(|_| AppError::create_output_dir(output_dir))?;

        let graph_path = output_dir.join("graph.json");
        let graph_json = CytoscapeJsonProjector::project(graph)?;
        fs::write(&graph_path, graph_json).map_err(|_| AppError::write_output(graph_path))
    }
}

fn document_id_from_path(path: &Path) -> String {
    let display_name = path
        .file_stem()
        .or_else(|| path.file_name())
        .map_or_else(|| String::from("document"), |value| {
            value.to_string_lossy().into_owned()
        });

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

    use super::{FileDocumentSource, FileGraphArtifactSink, document_id_from_path};
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
    fn writes_graph_json_to_output_directory() {
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
        assert!(graph_path.is_file());
        assert_eq!(
            fs::read_to_string(graph_path).expect("graph json"),
            r#"{"nodes":[],"edges":[]}"#
        );
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
}
