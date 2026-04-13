use crate::application::{AppError, Chunk, ExtractionOutcome};
use crate::domain::Document;

pub(crate) trait ChunkExtractor {
    fn extract(&self, chunk: Chunk) -> Result<ExtractionOutcome, AppError>;
}

pub(crate) trait DocumentPartitioner {
    fn partition(&self, document: &Document) -> Result<Vec<Chunk>, AppError>;
}
