use serde::Serialize;

use super::EpistemicStatus;
use super::Fact;
use super::TextUnit;

/// A factual claim that supports a proposed relationship between a source and target node.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct FactualClaim {
    /// The factual claim
    pub(crate) fact: Fact,

    /// The supporting quote returned by the extractor.
    pub(crate) citation_text: String,

    /// The source text unit
    // TODO should this be a reference or owned?
    pub(crate) citation: TextUnit,

    /// The degree of confidence in the claim.
    pub(crate) status: EpistemicStatus,
}
