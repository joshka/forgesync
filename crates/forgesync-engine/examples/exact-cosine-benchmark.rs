use std::time::Instant;

use forgesync_core::EmbeddingVector;
use forgesync_engine::cosine_similarity;

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
