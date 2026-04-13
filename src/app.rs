use std::io::{Read, Write};

use crate::adapters::{
    CytoscapeJsonProjector, ParallelChunkExtractor, ReadDocumentSource, SchemaLlmClient,
    TokenizerSource, WriteGraphSink,
};
use crate::application::{AppError, IngestConfig, IngestDocumentService, MaxConcurrency};

pub struct App<R, W, C, T> {
    config: IngestConfig,
    input_reader: R,
    cytoscape_writer: W,
    llm_client: C,
    tokenizer_source: T,
    max_concurrency: MaxConcurrency,
}

impl<R, W, C, T> App<R, W, C, T>
where
    R: Read,
    W: Write,
    C: SchemaLlmClient + 'static,
    T: TokenizerSource,
{
    pub fn new(
        config: IngestConfig,
        input_reader: R,
        cytoscape_writer: W,
        max_concurrency: MaxConcurrency,
        llm_client: C,
        tokenizer_source: T,
    ) -> Self {
        Self {
            config,
            input_reader,
            cytoscape_writer,
            llm_client,
            tokenizer_source,
            max_concurrency,
        }
    }

    pub fn run(self) -> Result<(), AppError> {
        let source = ReadDocumentSource::new(self.input_reader);
        let sink = WriteGraphSink::new(self.cytoscape_writer);
        let extractor = ParallelChunkExtractor::new(
            self.config,
            self.max_concurrency,
            self.llm_client,
            self.tokenizer_source,
        )?;
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

    use tokenizers::Tokenizer;
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;

    use super::App;
    use crate::adapters::{FakeSchemaLlmClient, StaticTokenizerSource};
    use crate::application::{IngestConfig, MaxConcurrency};

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
        let config = IngestConfig {
            tokenizer_name: String::from("test-wordlevel"),
            max_chunk_tokens: 32,
        };
        let app = App::new(
            config,
            input,
            output.clone(),
            MaxConcurrency(1),
            FakeSchemaLlmClient,
            StaticTokenizerSource::new(build_test_tokenizer()),
        );

        app.run().expect("app run succeeds");

        let json = String::from_utf8(output.0.borrow().clone()).expect("utf8 output");
        assert!(json.contains("\"id\":\"node:apple\""));
        assert!(json.contains("\"id\":\"node:fruit\""));
        assert!(json.contains("\"id\":\"node:trees\""));
        assert!(json.contains("\"edge_type\":\"IsA\""));
        assert!(json.contains("\"edge_type\":\"GrowsOn\""));
    }

    #[test]
    fn runs_end_to_end_for_multi_chunk_document() {
        let input = Cursor::new(String::from(
            "An apple is a red fruit that grows on trees.\n\nAn apple is a red fruit that grows on trees.",
        ));
        let output = SharedBuffer(Rc::new(RefCell::new(Vec::new())));
        let config = IngestConfig {
            tokenizer_name: String::from("test-wordlevel"),
            max_chunk_tokens: 12,
        };
        let app = App::new(
            config,
            input,
            output.clone(),
            MaxConcurrency(2),
            FakeSchemaLlmClient,
            StaticTokenizerSource::new(build_test_tokenizer()),
        );

        app.run().expect("app run succeeds");

        let json = String::from_utf8(output.0.borrow().clone()).expect("utf8 output");
        assert!(json.contains("\"weight\":2"));
    }

    fn build_test_tokenizer() -> Tokenizer {
        let vocab = [
            ("[UNK]".to_owned(), 0),
            ("an".to_owned(), 1),
            ("apple".to_owned(), 2),
            ("is".to_owned(), 3),
            ("a".to_owned(), 4),
            ("red".to_owned(), 5),
            ("fruit".to_owned(), 6),
            ("that".to_owned(), 7),
            ("grows".to_owned(), 8),
            ("on".to_owned(), 9),
            ("trees".to_owned(), 10),
            (".".to_owned(), 11),
        ]
        .into_iter()
        .collect();

        let model = WordLevel::builder()
            .vocab(vocab)
            .unk_token("[UNK]".into())
            .build()
            .expect("word level model");
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace));
        tokenizer
    }
}
