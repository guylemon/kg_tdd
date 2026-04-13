use std::io::{Read, Write};

use crate::adapters::{
    CytoscapeJsonProjector, ParallelChunkExtractor, ReadDocumentSource, SchemaLlmClient,
    WriteGraphSink,
};
use crate::application::{AppError, IngestDocumentService, MaxConcurrency};

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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::{Cursor, Write};
    use std::rc::Rc;

    use super::App;
    use crate::adapters::FakeSchemaLlmClient;
    use crate::application::MaxConcurrency;

    #[derive(Clone)]
    struct SharedBuffer(Rc<RefCell<Vec<u8>>>);

    impl Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn runs_end_to_end_for_seed_document() {
        let input = Cursor::new(String::from("An apple is a red fruit that grows on trees"));
        let output = SharedBuffer(Rc::new(RefCell::new(Vec::new())));
        let app = App::new(input, output.clone(), MaxConcurrency(1), FakeSchemaLlmClient);

        app.run().expect("app run succeeds");

        let json = String::from_utf8(output.0.borrow().clone()).expect("utf8 output");
        assert!(json.contains("\"id\":\"node:apple\""));
        assert!(json.contains("\"id\":\"node:fruit\""));
        assert!(json.contains("\"id\":\"node:trees\""));
        assert!(json.contains("\"edge_type\":\"IsA\""));
        assert!(json.contains("\"edge_type\":\"GrowsOn\""));
    }
}
