use serde::Serialize;

/// The supported Entity types for this application
// TODO remove unused allow rule
#[derive(Clone, Debug, Serialize)]
pub(crate) enum EntityType {
    /// Theoretical ideas, methodologies, approaches
    #[allow(unused)]
    Concept,

    /// Conferences, releases, historical events
    #[allow(unused)]
    Event,

    /// A biological lifeform, such as a plant, animal, insect. Given the current context, it is not a
    /// product.
    #[allow(unused)]
    Lifeform,

    /// Cities, countries, regions
    #[allow(unused)]
    Location,

    /// Companies, institutions, or universities
    #[allow(unused)]
    Organization,

    /// People mentioned in the chunk text
    #[allow(unused)]
    Person,

    /// Software products, platforms, services
    #[allow(unused)]
    Product,

    /// Frameworks, programming languages, algorithms
    #[allow(unused)]
    Technology,
}
