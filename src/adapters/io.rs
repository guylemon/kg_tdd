use std::io::{Read, Write};

use crate::application::AppError;
use crate::domain::{Document, DocumentId, NonEmptyString};

const TODO_STRING: &str = "";

pub(crate) struct ReadDocumentSource<R> {
    reader: R,
}

impl<R> ReadDocumentSource<R>
where
    R: Read,
{
    pub(crate) fn new(reader: R) -> Self {
        Self { reader }
    }

    pub(crate) fn read_document(mut self) -> Result<Document, AppError> {
        let mut raw_text = String::new();
        self.reader
            .read_to_string(&mut raw_text)
            .map_err(|_| AppError::ReadInput)?;

        Ok(Document {
            id: DocumentId(TODO_STRING.to_owned()),
            text: NonEmptyString(raw_text),
        })
    }
}

pub(crate) struct WriteGraphSink<W> {
    writer: W,
}

impl<W> WriteGraphSink<W>
where
    W: Write,
{
    pub(crate) fn new(writer: W) -> Self {
        Self { writer }
    }

    pub(crate) fn write_graph(mut self, graph_json: &str) -> Result<(), AppError> {
        self.writer
            .write_all(graph_json.as_bytes())
            .map_err(|_| AppError::WriteOutput)
    }
}
