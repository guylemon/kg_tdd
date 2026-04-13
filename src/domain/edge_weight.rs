use serde::Serialize;

// TODO It is unclear which type of number to use at this time.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct EdgeWeight(pub(crate) u16);
