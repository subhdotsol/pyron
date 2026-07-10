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
    /// Profile compute unit usage of your program
    Profile {
        /// Specific instruction to profile (profiles all if omitted)
        #[arg(short, long)]
        instruction: Option<String>,

        /// Number of simulation runs (default: 50)
        #[arg(short, long, default_value = "50")]
        runs: usize,

        /// RPC URL (default: localhost)
        #[arg(short = 'u', long, default_value = "http://127.0.0.1:8899")]
        rpc_url: String,
    },

    /// Save current profile as baseline for regression detection
    Baseline,

    /// Compare current profile against saved baseline
    Diff {
        #[arg(long, default_value = "10")]
        max_regression: f64,
    },

    /// Compare two compiled .so files side by side
    Compare { a: String, b: String },

    /// Get AI-powered optimization suggestions (requires ANTHROPIC_API_KEY)
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
        } => {
            println!("pyron profile");
            println!("   runs:    {}", runs);
            println!("   rpc:     {}", rpc_url);
            println!("   filter:  {}", instruction.as_deref().unwrap_or("all"));
            println!("\n[Phase 3 will implement the full profiling logic here]");
        }
        Commands::Baseline => println!("[Phase 4] baseline saved"),
        Commands::Diff { max_regression } => {
            println!("[Phase 4] diff — threshold {}%", max_regression)
        }
        Commands::Compare { a, b } => println!("[Phase 4] compare {} vs {}", a, b),
        Commands::Suggest => println!("[Phase 5] AI suggestions"),
    }

    Ok(())
}
