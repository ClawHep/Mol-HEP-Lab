//! Query generator — produces GitHub search queries from a research topic.

use tracing::debug;

// ---------------------------------------------------------------------------
// QueryGenerator
// ---------------------------------------------------------------------------

/// Generates a ranked list of GitHub search queries for a given research topic.
///
/// Uses simple heuristic expansion when no LLM is available.
pub struct QueryGenerator {
    /// Maximum number of queries to produce.
    pub max_queries: usize,
}

impl QueryGenerator {
    /// Create a new generator.
    pub fn new(max_queries: usize) -> Self {
        Self { max_queries }
    }

    /// Generate search queries for `topic` in `domain` using `core_libraries`.
    ///
    /// The returned list is ordered from most-specific to most-general.
    pub fn generate(
        &self,
        topic: &str,
        domain_name: &str,
        core_libraries: &[String],
        specific_needs: &[String],
    ) -> Vec<String> {
        debug!(topic, domain_name, "Generating search queries");
        let mut queries: Vec<String> = Vec::new();

        // Most specific: topic + primary library
        if let Some(lib) = core_libraries.first() {
            queries.push(format!("{topic} {lib}"));
        }

        // Topic alone
        queries.push(topic.to_owned());

        // Domain + first specific need
        for need in specific_needs.iter().take(2) {
            queries.push(format!("{domain_name} {need}"));
        }

        // Core libraries individually
        for lib in core_libraries.iter().take(3) {
            let q = format!("{lib} {}", short_topic(topic));
            if !queries.contains(&q) {
                queries.push(q);
            }
        }

        // Generic domain query as fallback
        let domain_q = format!("{domain_name} Python implementation");
        if !queries.contains(&domain_q) {
            queries.push(domain_q);
        }

        queries.truncate(self.max_queries);
        queries
    }
}

/// Return the first N words of a topic string (for short sub-queries).
fn short_topic(topic: &str) -> String {
    topic.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Free function (convenience wrapper matching Python's generate_search_queries)
// ---------------------------------------------------------------------------

/// Generate search queries — free-function convenience wrapper.
pub fn generate_search_queries(
    topic: &str,
    domain_name: &str,
    core_libraries: &[String],
    specific_needs: &[String],
    max_queries: usize,
) -> Vec<String> {
    QueryGenerator::new(max_queries).generate(topic, domain_name, core_libraries, specific_needs)
}
