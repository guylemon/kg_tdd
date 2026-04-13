use crate::domain::Todo;

#[derive(Debug)]
pub(crate) enum AppError {
    ReadInput,
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
