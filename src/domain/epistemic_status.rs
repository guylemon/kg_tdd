use serde::Serialize;

/// Represents the degree of epistemic certainty for a given claim, given the context.
// TODO remove unused allow rule
#[derive(Debug, Serialize)]
pub(crate) enum EpistemicStatus {
    /// An arbitrary claim has no perceptual or conceptual evidence. It is neither true or false
    /// because it is outside of cognition. The arbitrary is distinct from the possible because the
    /// arbitrary has no proposed evidence.
    ///
    /// Examples:
    /// - "There is a teapot orbiting Mars."
    /// - "You are secretly a parrot."
    #[allow(unused)]
    Arbitrary,

    /// A claim for which there is some, but not conclusive evidence. There is nothing in the
    /// current context of knowledge that contradicts the evidence.
    #[allow(unused)]
    Possible,

    /// A claim for which the evidence is strong, but not yet conclusive. The context indicates
    /// that the weight of the evidence supports the claim, making it more likely to be true than
    /// false; however, further evidence could still tip the scale.
    #[allow(unused)]
    Probable,

    /// A claim for which the evidence is conclusive within a given context of knowledge. All
    /// available evidence supports the claim, and there is no evidence to support any alternative.
    #[allow(unused)]
    Certain,
}
