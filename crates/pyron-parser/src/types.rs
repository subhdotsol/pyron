use serde::{Deserialize, Serialize};

/// One frame in the CPI call tree.
/// Represents a single program invocation and all its nested calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuNode {
    /// Short program name or address
    pub program: String,

    /// Instruction name from "Program log: Instruction: X"
    /// None if the program didn't emit an instruction log
    pub instruction: Option<String>,

    /// CPI depth — 1 = top level, 2 = called from top level, etc.
    pub depth: usize,

    /// Total CU consumed by this frame including all children
    pub cu_consumed: u64,

    /// CU consumed by this frame ONLY — excludes children
    /// cu_self = cu_consumed - sum(children cu_consumed)
    pub cu_self: u64,

    /// Nested CPI calls made by this program
    pub children: Vec<CuNode>,

    /// Whether this frame succeeded or failed
    pub success: bool,
}

impl CuNode {
    /// Total CU percentage of the 1.4M hard cap
    pub fn budget_pct(&self) -> f64 {
        (self.cu_consumed as f64 / 1_400_000.0) * 100.0
    }

    /// Percentage this node is of its parent's total
    pub fn pct_of(&self, parent_cu: u64) -> f64 {
        if parent_cu == 0 {
            return 0.0;
        }
        (self.cu_consumed as f64 / parent_cu as f64) * 100.0
    }
}

/// Statistics for one instruction across N simulation runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuStats {
    pub instruction: String,
    pub p50: u64, // median
    pub p95: u64, // 95th percentile — use for ComputeBudget
    pub max: u64, // worst case
    pub min: u64,
    pub runs: usize, // how many simulations ran

    /// Suggested setComputeUnitLimit value: p95 + 10% buffer
    pub recommended_cu_limit: u64,
}

/// The full report for one profiling run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuReport {
    pub program_id: String,
    pub timestamp: String,
    /// One CuStats per instruction
    pub stats: Vec<CuStats>,
    /// The full CPI tree from one representative run (p50 run)
    pub tree: Option<CuNode>,
}
