//! # Interpret explicit discussion references
//!
//! Thread mentions in titles and bodies can support a duplicate edge independently of vectors.
//! References must name this repository and an available non-self target. Title mentions and early
//! body mentions have stronger context; later body mentions need title-token overlap.
//!
//! The output maps ordered document-index pairs to reference confidence. `evidence` combines these
//! scores with semantic similarity. This module does not choose graph components or persist groups.
//! Stable input ordering belongs to `candidates`; mention matching is local and deterministic.
//!
//! Title tokens also provide the evidence guard against weak semantic matches.

use super::{
    EARLY_BODY_REFERENCE_BYTES, EmbeddingSearchDocument, HashMap, HashSet, MIN_TITLE_OVERLAP,
    REFERENCE_SCORE, THREAD_REFERENCE, TITLE_TOKEN,
};

/// Adds stable reference-based links independently of vector ranking.
pub fn deterministic_reference_edges(
    documents: &[EmbeddingSearchDocument],
    repository_full_name: &str,
) -> HashMap<(usize, usize), f64> {
    let by_number = documents
        .iter()
        .enumerate()
        .map(|(index, document)| (document.summary.discussion.id.number().get(), index))
        .collect::<HashMap<_, _>>();
    let titles = documents
        .iter()
        .map(|document| title_tokens(&document.summary.discussion.title))
        .collect::<Vec<_>>();
    let mut edges = HashMap::new();
    for (source_index, document) in documents.iter().enumerate() {
        let discussion = &document.summary.discussion;
        let mut collector = ReferenceCollector {
            edges: &mut edges,
            source_index,
            source_number: discussion.id.number().get(),
            repository: repository_full_name,
            by_number: &by_number,
            titles: &titles,
        };
        collector.collect(ReferenceText::Title(&discussion.title));
        if let Some(body) = discussion.body.as_deref() {
            collector.collect(ReferenceText::Body(body));
        }
    }
    edges
}

/// Location of a mention, which determines the supporting-context requirement.
enum ReferenceText<'a> {
    Title(&'a str),
    Body(&'a str),
}
impl ReferenceText<'_> {
    /// Borrows text while retaining its location for eligibility policy.
    fn text(&self) -> &str {
        match self {
            Self::Title(text) | Self::Body(text) => text,
        }
    }
}

/// Reference lookup context for one source discussion.
struct ReferenceCollector<'a> {
    edges: &'a mut HashMap<(usize, usize), f64>,
    source_index: usize,
    source_number: u64,
    repository: &'a str,
    by_number: &'a HashMap<u64, usize>,
    titles: &'a [HashSet<String>],
}
impl ReferenceCollector<'_> {
    /// Adds eligible non-self mentions with the context required by their location.
    fn collect(&mut self, source: ReferenceText<'_>) {
        for captures in THREAD_REFERENCE.captures_iter(source.text()) {
            let referenced_repository = captures.get(1).or_else(|| captures.get(3));
            if referenced_repository.is_some_and(|repository| {
                !repository.as_str().eq_ignore_ascii_case(self.repository)
            }) {
                continue;
            }
            let number_capture = captures
                .get(2)
                .or_else(|| captures.get(4))
                .or_else(|| captures.get(5));
            let Some(number) = number_capture.and_then(|value| value.as_str().parse::<u64>().ok())
            else {
                continue;
            };
            if number == self.source_number {
                continue;
            }
            let Some(&target_index) = self.by_number.get(&number) else {
                continue;
            };
            let early_body = matches!(source, ReferenceText::Body(_))
                && captures
                    .get(0)
                    .is_some_and(|reference| reference.start() <= EARLY_BODY_REFERENCE_BYTES);
            if matches!(source, ReferenceText::Body(_))
                && !early_body
                && overlap_ratio(&self.titles[self.source_index], &self.titles[target_index])
                    < MIN_TITLE_OVERLAP
            {
                continue;
            }
            self.edges
                .entry((
                    self.source_index.min(target_index),
                    self.source_index.max(target_index),
                ))
                .and_modify(|score| *score = score.max(REFERENCE_SCORE))
                .or_insert(REFERENCE_SCORE);
        }
    }
}

/// Extracts comparable title tokens for reference heuristics.
pub fn title_tokens(value: &str) -> HashSet<String> {
    TITLE_TOKEN
        .find_iter(value)
        .map(|token| token.as_str().to_ascii_lowercase())
        .collect()
}

/// Measures title-token overlap for candidate support.
pub fn overlap_ratio(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let overlap = left.intersection(right).count();
    overlap as f64 / left.len().min(right.len()) as f64
}
