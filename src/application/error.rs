use crate::domain::Todo;

#[derive(Debug)]
pub(crate) enum AppError {
    PartitionDocument,
    ReadInput,
    LoadTokenizer,
    ExtractChunk,
    ProjectGraph,
    WriteOutput,
    Internal,
}

impl From<Todo> for AppError {
    fn from(_: Todo) -> Self {
        Self::Internal
    }
}
