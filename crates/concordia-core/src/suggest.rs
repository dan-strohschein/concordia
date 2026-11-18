//! "Did you mean" suggestions for agent-facing errors.
//!
//! ```
//! use concordia_core::suggest::did_you_mean;
//! assert_eq!(did_you_mean("Transfrom", ["Transform", "Name"]), vec!["Transform"]);
//! ```

/// Minimum Jaro-Winkler similarity for a suggestion.
pub const MIN_SIMILARITY: f64 = 0.8;
/// Maximum number of suggestions.
pub const MAX_SUGGESTIONS: usize = 3;

/// Up to three candidates similar to `input` (Jaro-Winkler ≥ 0.8), best first; never `input` itself.
pub fn did_you_mean<'a>(
    input: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(f64, &'a str)> = candidates
        .into_iter()
        .filter(|c| *c != input)
        .map(|c| (strsim::jaro_winkler(input, c), c))
        .filter(|(score, _)| *score >= MIN_SIMILARITY)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    scored
        .into_iter()
        .take(MAX_SUGGESTIONS)
        .map(|(_, c)| c)
        .collect()
}
