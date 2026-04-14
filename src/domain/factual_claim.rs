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

impl FactualClaim {
    /// Accepted evidence is grounded in a source chunk and currently defaults to `Probable`.
    pub(crate) fn grounded(fact: String, citation_text: String, citation: TextUnit) -> Self {
        Self {
            fact: Fact(fact),
            citation_text,
            citation,
            status: EpistemicStatus::Probable,
        }
    }

    pub(crate) fn dedupe_key(&self) -> (String, String, String, String, usize) {
        (
            normalize_for_identity(&self.fact.0),
            normalize_for_identity(&self.citation_text),
            self.citation.document_id.0.clone(),
            self.citation.text.0.clone(),
            self.citation.token_count.0,
        )
    }
}

fn normalize_for_identity(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::FactualClaim;
    use crate::domain::{AnnotatedText, DocumentId, TextUnit, TokenCount};

    #[test]
    fn grounded_claim_defaults_to_probable_status() {
        let claim = FactualClaim::grounded(
            String::from("fact"),
            String::from("citation"),
            TextUnit {
                document_id: DocumentId(String::from("doc-1")),
                text: AnnotatedText(String::from("citation")),
                token_count: TokenCount(1),
            },
        );

        assert!(matches!(
            claim.status,
            crate::domain::EpistemicStatus::Probable
        ));
    }

    #[test]
    fn dedupe_key_normalizes_fact_and_citation_text() {
        let claim = FactualClaim::grounded(
            String::from("An   apple is   fruit"),
            String::from("apple   is fruit"),
            TextUnit {
                document_id: DocumentId(String::from("doc-1")),
                text: AnnotatedText(String::from("apple is fruit")),
                token_count: TokenCount(3),
            },
        );

        let key = claim.dedupe_key();

        assert_eq!(key.0, "An apple is fruit");
        assert_eq!(key.1, "apple is fruit");
    }
}
