mod cmd;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pyron", about = "Solana compute unit profiler", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Profile compute unit usage of your Anchor program
    Profile {
        /// Profile only this instruction (profiles all if omitted)
        #[arg(short, long)]
        instruction: Option<String>,

        /// Number of simulation runs per instruction
        /// How many simulations to run per instruction. More = better p50/p95/max accuracy.
        /// 10 is good for devnet; use 50+ on a local validator.
        #[arg(short, long, default_value = "10")]
        runs: usize,

        /// RPC URL (defaults to devnet)
        #[arg(short = 'u', long, default_value = "https://api.devnet.solana.com")]
        rpc_url: String,

        /// Override program ID (auto-detected from Anchor.toml if omitted)
        #[arg(short, long)]
        program_id: Option<String>,

        /// Override IDL path (auto-detected if omitted)
        #[arg(long)]
        idl: Option<String>,

        /// Fee payer pubkey for simulation (auto-detected from program upgrade authority if omitted)
        #[arg(long)]
        payer: Option<String>,
    },

    /// Save current profile as baseline for regression detection
    Baseline {
        #[arg(short = 'u', long, default_value = "https://api.devnet.solana.com")]
        rpc_url: String,

        #[arg(short, long)]
        program_id: Option<String>,

        #[arg(long)]
        idl: Option<String>,

        #[arg(short, long, default_value = "10")]
        runs: usize,

        /// Output file path
        #[arg(short, long, default_value = "pyron-baseline.json")]
        output: String,
    },

    /// Compare current profile against saved baseline
    Diff {
        #[arg(short = 'u', long, default_value = "https://api.devnet.solana.com")]
        rpc_url: String,

        #[arg(short, long)]
        program_id: Option<String>,

        #[arg(long)]
        idl: Option<String>,

        #[arg(short, long, default_value = "10")]
        runs: usize,

        /// Fail if any instruction regresses more than this %
        #[arg(long, default_value = "10")]
        max_regression: f64,

        /// Path to baseline file
        #[arg(short, long, default_value = "pyron-baseline.json")]
        baseline: String,
    },

    /// Compare two .so files side by side
    Compare { a: String, b: String },

    /// AI-powered optimization suggestions
    Suggest,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Profile {
            instruction,
            runs,
            rpc_url,
            program_id,
            idl,
            payer,
        } => {
            cmd::profile::run(instruction, runs, rpc_url, program_id, idl, payer)?;
        }
        Commands::Baseline {
            rpc_url,
            program_id,
            idl,
            runs,
            output,
        } => {
            cmd::baseline::run(rpc_url, program_id, idl, runs, output)?;
        }
        Commands::Diff {
            rpc_url,
            program_id,
            idl,
            runs,
            max_regression,
            baseline,
        } => {
            let exit_code =
                cmd::diff::run(rpc_url, program_id, idl, runs, max_regression, baseline)?;
            std::process::exit(exit_code);
        }
        Commands::Compare { a, b } => println!("[Phase 4] compare {} vs {}", a, b),
        Commands::Suggest => println!("[Phase 5] suggest — coming soon"),
    }

    Ok(())
}
