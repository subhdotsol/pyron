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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CuNode;

    fn make_node(cu: u64) -> CuNode {
        CuNode {
            program: "vault".to_string(),
            instruction: Some("deposit".to_string()),
            depth: 1,
            cu_consumed: cu,
            cu_self: cu,
            children: vec![],
            success: true,
            logs: vec![],
            budget_at_invocation: 200_000,
        }
    }

    #[test]
    fn test_percentiles_vary() {
        // 21 runs: p95 idx=19 (48_500), max idx=20 (52_000) — they differ
        let cu_values = vec![
            44_800, 45_000, 45_100, 45_200, 45_300,
            45_400, 45_500, 45_600, 45_700, 45_800,
            46_000, 46_200, 46_500, 46_800, 47_000,
            47_200, 47_500, 48_000, 48_500, 49_000,
            52_000, // one outlier above p95
        ];
        let runs: Vec<CuNode> = cu_values.into_iter().map(make_node).collect();

        let stats = aggregate(runs);

        println!("p50: {}  p95: {}  max: {}  recommended: {}",
            stats.p50, stats.p95, stats.max, stats.recommended_cu_limit);

        assert!(stats.p50 < stats.p95, "p50 should be less than p95");
        assert!(stats.p95 < stats.max, "p95 should be less than max");
        assert_eq!(stats.max, 52_000);
        assert_eq!(stats.recommended_cu_limit, (stats.p95 as f64 * 1.10) as u64);
    }

    #[test]
    fn test_single_run_all_same() {
        // confirms the actual behaviour you observed: 1 run → p50 = p95 = max
        let runs = vec![make_node(45_230)];
        let stats = aggregate(runs);

        assert_eq!(stats.p50, 45_230);
        assert_eq!(stats.p95, 45_230);
        assert_eq!(stats.max, 45_230);
    }
}
