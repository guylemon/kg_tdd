use std::path::Path;

use crate::application::{AppError, Chunk, ExtractionOutcome};
use crate::domain::{Document, KnowledgeGraph};

pub(crate) trait ChunkExtractor {
    fn extract(&self, chunk: Chunk) -> Result<ExtractionOutcome, AppError>;
}

pub(crate) trait DocumentPartitioner {
    fn partition(&self, document: &Document) -> Result<Vec<Chunk>, AppError>;
}

pub(crate) trait DocumentSource {
    fn read_document(&self, input_path: &Path) -> Result<Document, AppError>;
}

pub(crate) trait GraphArtifactSink {
    fn write_graph(&self, output_dir: &Path, graph: &KnowledgeGraph) -> Result<(), AppError>;
}
