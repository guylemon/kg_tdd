use std::io::{Read, Write};

use crate::adapters::{
    CytoscapeJsonProjector, ParallelChunkExtractor, ReadDocumentSource, SchemaLlmClient,
    WriteGraphSink,
};
use crate::application::{AppError, IngestDocumentService, MaxConcurrency};

pub use crate::domain::Todo;

pub struct App<R, W, C> {
    input_reader: R,
    cytoscape_writer: W,
    llm_client: C,
    max_concurrency: MaxConcurrency,
}

impl<R, W, C> App<R, W, C>
where
    R: Read,
    W: Write,
    C: SchemaLlmClient + 'static,
{
    pub fn new(
        input_reader: R,
        cytoscape_writer: W,
        max_concurrency: MaxConcurrency,
        llm_client: C,
    ) -> Self {
        Self {
            input_reader,
            cytoscape_writer,
            llm_client,
            max_concurrency,
        }
    }

    pub fn run(self) -> Result<(), AppError> {
        let source = ReadDocumentSource::new(self.input_reader);
        let sink = WriteGraphSink::new(self.cytoscape_writer);
        let extractor = ParallelChunkExtractor::new(self.max_concurrency, self.llm_client);
        let service = IngestDocumentService::new(extractor);

        let document = source.read_document()?;
        let knowledge_graph = service.execute(&document)?;
        let cytoscape_json = CytoscapeJsonProjector::project(&knowledge_graph)?;
        sink.write_graph(&cytoscape_json)?;

        Ok(())
    }
}
