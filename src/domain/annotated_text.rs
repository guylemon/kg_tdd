use serde::Serialize;

/// ``AnnotatedText`` is text from a Chunk that has its entities marked.
///
/// - Raw text -> "I like apples"
/// - Annotated text -> "<entity type=Person>I</entity> like <entity type=Lifeform>apples</entity>"
#[derive(Clone, Debug, Serialize)]
pub(crate) struct AnnotatedText(pub(crate) String);
