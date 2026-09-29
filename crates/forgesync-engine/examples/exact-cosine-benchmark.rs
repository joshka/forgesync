//! # Measure exact vector scoring and selection
//!
//! This example is a small performance probe for the engine's `cosine_similarity` primitive. It
//! helps maintainers measure whether straightforward exact scoring is practical at a chosen vector
//! count and dimension before changing the representation or adding a more complex retrieval
//! strategy. It runs without an archive, network service, or downloaded model, making the
//! arithmetic and selection cost easier to investigate in isolation.
//!
//! ## What this demonstrates
//!
//! A fixed-seed pseudo-random generator constructs reproducible nonzero candidate vectors. The
//! query alternates positive and negative components. Both use the checked `EmbeddingVector`
//! constructor, so the scoring loop consumes the same vector type as the engine's semantic search
//! path.
//!
//! The timed region computes cosine similarity for every candidate, retains positive scores, sorts
//! all retained scores in descending order with candidate ID as the tie break, and keeps at most
//! twenty results. It therefore measures scoring plus result allocation and full sorting. Vector
//! generation and validation occur before the timer, while input-size calculation and printing
//! occur afterward.
//!
//! This synthetic loop demonstrates the cost of those operations, not the complete semantic-search
//! workflow. It excludes SQLite reads, document/chunk matching, model compatibility checks, query
//! embedding, and the engine's page-bounded search coordination. Use an actual archive workload
//! when investigating those costs or user-visible latency.
//!
//! ## Run it
//!
//! Build and run in release mode so debug arithmetic and collection overhead do not dominate:
//!
//! ```sh
//! cargo run --release -p forgesync-engine --example exact-cosine-benchmark -- 10000 1536
//! ```
//!
//! The optional positional arguments are candidate count and vector dimensions; their defaults are
//! 10,000 and 1,536. Both must be positive integers. Increasing either increases work, and
//! candidate storage grows approximately with their product. Choose sizes that fit the machine's
//! available memory before using this as a larger scaling experiment.
//!
//! ## Interpret the measurements
//!
//! `vectors` and `dimensions` describe the workload. `input_bytes` counts only the candidate `f32`
//! components; it excludes vector/container overhead, the query, and the score buffer. `elapsed_ms`
//! is one wall-clock measurement of the timed region. `top_k` is the number retained, which may be
//! less than twenty if too few candidates have positive similarity.
//!
//! Repeat the same invocation on the same machine when comparing changes, and record the build mode
//! and workload with the result. This executable does not warm up, run repeated samples, or
//! calculate statistical confidence. Small timing differences may be noise; sustained changes
//! should be checked with a controlled benchmark before they drive an architectural decision.

use std::time::Instant;

use forgesync_core::embedding::EmbeddingVector;
use forgesync_engine::exact_search::cosine_similarity;

/// Generates a reproducible workload, times scoring and selection, and prints one measurement.
fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    let count = arguments
        .get(1)
        .map_or(Ok(10_000_usize), |value| value.parse())
        .expect("count must be an integer");
    let dimensions = arguments
        .get(2)
        .map_or(Ok(1_536_usize), |value| value.parse())
        .expect("dimensions must be an integer");
    assert!(count > 0, "count must be positive");
    assert!(dimensions > 0, "dimensions must be positive");

    let mut state = 0x9e37_79b9_u32;
    let vectors = (0..count)
        .map(|_| {
            let values = (0..dimensions)
                .map(|_| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let unit = (state >> 8) as f32 / (1_u32 << 24) as f32;
                    unit * 2.0 - 1.0
                })
                .collect::<Vec<_>>();
            EmbeddingVector::new(values, None).expect("generated nonzero vector")
        })
        .collect::<Vec<_>>();
    let query_values = (0..dimensions)
        .map(|index| if index % 2 == 0 { 1.0 } else { -1.0 })
        .collect::<Vec<_>>();
    let query = EmbeddingVector::new(query_values, None).expect("query vector");

    let started = Instant::now();
    let mut scores = vectors
        .iter()
        .enumerate()
        .filter_map(|(id, vector)| {
            cosine_similarity(&query, vector)
                .filter(|score| *score > 0.0)
                .map(|score| (id, score))
        })
        .collect::<Vec<_>>();
    scores.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    scores.truncate(20);
    let elapsed = started.elapsed();
    let input_bytes = count
        .checked_mul(dimensions)
        .and_then(|components| components.checked_mul(std::mem::size_of::<f32>()))
        .expect("input size fits usize");

    println!(
        "vectors={count} dimensions={dimensions} input_bytes={input_bytes} elapsed_ms={:.3} top_k={}",
        elapsed.as_secs_f64() * 1_000.0,
        scores.len()
    );
}
