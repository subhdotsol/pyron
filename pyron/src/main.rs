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
        #[arg(short, long, default_value = "50")]
        runs: usize,

        /// RPC URL (defaults to localhost:8899)
        #[arg(short = 'u', long, default_value = "http://127.0.0.1:8899")]
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
    Baseline,

    /// Compare current profile against saved baseline
    Diff {
        #[arg(long, default_value = "10")]
        max_regression: f64,
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
        Commands::Baseline => println!("[Phase 4] baseline — coming soon"),
        Commands::Diff { max_regression } => {
            println!("[Phase 4] diff — threshold {}%", max_regression)
        }
        Commands::Compare { a, b } => println!("[Phase 4] compare {} vs {}", a, b),
        Commands::Suggest => println!("[Phase 5] suggest — coming soon"),
    }

    Ok(())
}
