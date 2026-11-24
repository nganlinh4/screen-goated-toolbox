use super::index::{ChunkEntry, SOURCE_DIMENSIONS};

const QUERY_PREFIX: &str = "task: question answering | query: ";
const TOP_K: usize = 6;

pub(super) fn retrieve<'a>(
    index: &'a [ChunkEntry],
    question: &str,
    key: &str,
) -> Vec<&'a ChunkEntry> {
    let is_source = index.first().is_some_and(|chunk| !chunk.vector.is_empty());
    if !is_source {
        return super::search_chunks(index, question);
    }
    match crate::api::gemini_embed::embed_text(
        &format!("{QUERY_PREFIX}{question}"),
        key,
        SOURCE_DIMENSIONS,
    ) {
        Ok(query) => rank(index, &query),
        Err(_) => super::search_chunks(index, question)
            .into_iter()
            .take(TOP_K)
            .collect(),
    }
}

fn rank<'a>(index: &'a [ChunkEntry], query: &[f32]) -> Vec<&'a ChunkEntry> {
    // The query norm is the same for every comparison.
    let mut scored: Vec<_> = index
        .iter()
        .map(|chunk| {
            let (dot, norm) = query.iter().zip(&chunk.vector).fold(
                (0.0_f64, 0.0_f64),
                |(dot, norm), (&query, &value)| {
                    let value = f64::from(value);
                    (dot + f64::from(query) * value, norm + value * value)
                },
            );
            (dot / norm.sqrt(), chunk)
        })
        .collect();
    scored.sort_by(|(left, _), (right, _)| right.total_cmp(left));
    scored
        .into_iter()
        .take(TOP_K)
        .map(|(_, chunk)| chunk)
        .collect()
}
