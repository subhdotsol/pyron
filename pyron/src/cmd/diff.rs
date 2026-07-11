use anyhow::{Context, Result};
use pyron_parser::{aggregate, diff_against_baseline, Baseline};
use pyron_report::{print_diff_header, print_diff_row, print_diff_summary};
use pyron_sim::{
    detect_idl, detect_program_id, load_idl, resolve_payer_pubkey, simulate_instruction, RunConfig,
};
use std::path::PathBuf;

pub fn run(
    rpc_url: String,
    program_id_override: Option<String>,
    idl_path_override: Option<String>,
    runs: usize,
    max_regression: f64,
    baseline_path: String,
) -> Result<i32> {
    let cwd = std::env::current_dir()?;

    let baseline =
        Baseline::load(&PathBuf::from(&baseline_path)).map_err(|e| anyhow::anyhow!(e))?;

    let program_id = match program_id_override {
        Some(ref id) => id.parse().context("Invalid program ID")?,
        None => detect_program_id(&cwd)
            .context("Could not detect program ID. Pass --program-id or run from your project root.")?,
    };

    let idl_path = match idl_path_override {
        Some(ref p) => PathBuf::from(p),
        None => detect_idl(&cwd).context("No IDL found. Run `anchor build` first, or pass --idl")?,
    };

    let instructions = load_idl(&idl_path)?;
    let payer_pubkey = resolve_payer_pubkey(&rpc_url, &program_id)
        .unwrap_or_default();
    let config = RunConfig { rpc_url, runs, program_id, payer_pubkey };

    let mut current_stats = Vec::new();
    for ix in &instructions {
        if let Ok(nodes) = simulate_instruction(&config, ix) {
            current_stats.push(aggregate(nodes));
        }
    }

    let results = diff_against_baseline(&baseline, &current_stats, max_regression);

    print_diff_header(&baseline_path, &baseline.saved_at, max_regression);
    for r in &results {
        print_diff_row(r);
    }
    let exit_code = print_diff_summary(&results);

    Ok(exit_code)
}
