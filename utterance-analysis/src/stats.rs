//! Order statistics shared by analysis and the mapping layer.

/// The upper median — the middle element once sorted — or `None` when empty.
/// Sorted with a total order, so a NaN is filed at an end rather than panicking.
pub fn median(values: &[f32]) -> Option<f32> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted.get(sorted.len() / 2).copied()
}
