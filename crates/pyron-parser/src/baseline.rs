use crate::types::CuStats;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

/// The saved baseline file — one p95 value per instruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    /// instruction name → p95 CU
    pub instructions: HashMap<String, u64>,
    /// when this baseline was saved
    pub saved_at: String,
    /// program ID this baseline is for
    pub program_id: String,
}

impl Baseline {
    /// Create a baseline from a list of CuStats
    pub fn from_stats(stats: &[CuStats], program_id: &str) -> Self {
        let mut instructions = HashMap::new();
        for s in stats {
            instructions.insert(s.instruction.clone(), s.p95);
        }
        Self {
            instructions,
            saved_at: now_string(),
            program_id: program_id.to_string(),
        }
    }

    /// Save baseline to a JSON file
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Load baseline from a JSON file
    pub fn load(path: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(path).map_err(|_| {
            format!(
                "baseline file not found at {}\nRun `pyron baseline` first.",
                path.display()
            )
        })?;
        serde_json::from_str(&contents).map_err(|e| e.to_string())
    }
}

/// One instruction's diff result
#[derive(Debug, Clone)]
pub struct DiffResult {
    pub instruction: String,
    pub baseline_cu: u64,
    pub current_cu: u64,
    /// percentage change — positive = regression, negative = improvement
    pub delta_pct: f64,
    pub is_regression: bool,
}

/// Compare current stats against a baseline
pub fn diff_against_baseline(
    baseline: &Baseline,
    current: &[CuStats],
    max_regression_pct: f64,
) -> Vec<DiffResult> {
    current
        .iter()
        .map(|s| {
            let baseline_cu = *baseline.instructions.get(&s.instruction).unwrap_or(&s.p95); // new instruction — use itself as baseline

            let delta_pct = if baseline_cu == 0 {
                0.0
            } else {
                ((s.p95 as f64 - baseline_cu as f64) / baseline_cu as f64) * 100.0
            };

            DiffResult {
                instruction: s.instruction.clone(),
                baseline_cu,
                current_cu: s.p95,
                delta_pct,
                is_regression: delta_pct > max_regression_pct,
            }
        })
        .collect()
}

fn now_string() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", secs)
}
