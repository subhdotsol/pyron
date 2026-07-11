use anyhow::{Context, Result};
use colored::Colorize;
use pyron_parser::{aggregate, Baseline};
use pyron_sim::{detect_idl, detect_program_id, load_idl, resolve_payer_pubkey, simulate_instruction, RunConfig};
use solana_sdk::pubkey::Pubkey;
use std::path::PathBuf;

pub fn run(
    rpc_url: String,
    program_id_override: Option<String>,
    idl_path_override: Option<String>,
    runs: usize,
    output: String,
) -> Result<()> {
    let cwd = std::env::current_dir()?;

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
        .unwrap_or(Pubkey::default());
    let config = RunConfig {
        rpc_url,
        runs,
        program_id,
        payer_pubkey,
    };

    println!("\n{} running baseline profile...\n", "pyron".cyan().bold());

    let mut all_stats = Vec::new();

    for ix in &instructions {
        print!("  profiling {}...", ix.name.white());
        use std::io::Write;
        std::io::stdout().flush().ok();

        match simulate_instruction(&config, ix) {
            Ok(nodes) => {
                let stats = aggregate(nodes);
                println!(" {} CU", stats.p95.to_string().cyan());
                all_stats.push(stats);
            }
            Err(e) => println!(" ✗ {}", e),
        }
    }

    // save the baseline
    let baseline = Baseline::from_stats(&all_stats, &program_id.to_string());
    let output_path = PathBuf::from(&output);
    baseline
        .save(&output_path)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!();
    println!("{} baseline saved to {}", "✓".green().bold(), output.cyan());
    println!(
        "{} commit {} to git to track regressions across PRs",
        "tip:".dimmed(),
        output.dimmed()
    );
    println!();

    Ok(())
}

