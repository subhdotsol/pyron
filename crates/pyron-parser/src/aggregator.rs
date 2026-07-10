use crate::types::{CuNode, CuStats};

/// Given N CuNode trees (one per simulation run),
/// compute p50/p95/max statistics for the root instruction.
pub fn aggregate(runs: Vec<CuNode>) -> CuStats {
    let instruction = runs
        .first()
        .and_then(|n| n.instruction.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let mut values: Vec<u64> = runs.iter().map(|n| n.cu_consumed).collect();
    values.sort_unstable();

    let runs_count = values.len();
    let p50 = percentile(&values, 50);
    let p95 = percentile(&values, 95);
    let max = *values.last().unwrap_or(&0);
    let min = *values.first().unwrap_or(&0);

    // Recommended limit: p95 + 10% safety buffer
    let recommended_cu_limit = (p95 as f64 * 1.10) as u64;

    CuStats {
        instruction,
        p50,
        p95,
        max,
        min,
        runs: runs_count,
        recommended_cu_limit,
    }
}

/// Get the value at the given percentile from a sorted slice.
fn percentile(sorted: &[u64], pct: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (pct * sorted.len() / 100).min(sorted.len() - 1);
    sorted[idx]
}
