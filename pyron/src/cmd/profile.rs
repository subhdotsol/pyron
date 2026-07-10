use anyhow::{Context, Result};
use pyron_parser::aggregate;
use pyron_report::{
    clear_progress, print_footer, print_header, print_instruction_stats, print_simulating,
};
use pyron_sim::{detect, load_idl, simulate_instruction, RunConfig};
use solana_sdk::pubkey::Pubkey;
use std::{path::PathBuf, time::Instant};

pub fn run(
    instruction_filter: Option<String>,
    runs: usize,
    rpc_url: String,
    program_id_override: Option<String>,
    idl_path_override: Option<String>,
) -> Result<()> {
    let start = Instant::now();
    let cwd = std::env::current_dir()?;

    let project =
        detect(&cwd).context("Could not detect Solana project. Run from your project root.")?;

    let program_id = match program_id_override {
        Some(ref id) => id
            .parse()
            .context("Invalid program ID — should be a base58 pubkey")?,
        None => read_program_id_from_anchor_toml(&cwd).unwrap_or_default(),
    };

    let idl_path = match idl_path_override {
        Some(ref p) => PathBuf::from(p),
        None => project
            .idl_path
            .clone()
            .context("No IDL found. Run `anchor build` first, or pass --idl")?,
    };

    let instructions = load_idl(&idl_path)
        .with_context(|| format!("Could not load IDL from {}", idl_path.display()))?;

    let instructions: Vec<_> = match &instruction_filter {
        Some(name) => instructions
            .into_iter()
            .filter(|ix| ix.name.to_lowercase().contains(&name.to_lowercase()))
            .collect(),
        None => instructions,
    };

    if instructions.is_empty() {
        anyhow::bail!("No instructions found matching filter. Check your IDL.");
    }

    let config = RunConfig {
        rpc_url: rpc_url.clone(),
        runs,
        program_id,
        payer_pubkey: Pubkey::default(),
    };

    print_header(&program_id.to_string(), &rpc_url, runs);

    let mut profiled = 0;

    for ix in &instructions {
        for i in 0..runs {
            print_simulating(&ix.name, i, runs);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        match simulate_instruction(&config, ix) {
            Ok(nodes) => {
                clear_progress();
                let stats = aggregate(nodes.clone());
                let tree = nodes.first();
                print_instruction_stats(&stats, tree);
                profiled += 1;
            }
            Err(e) => {
                clear_progress();
                eprintln!("  error: {} {}", ix.name, e);
            }
        }
    }

    print_footer(profiled, start.elapsed().as_secs_f64());

    Ok(())
}

fn read_program_id_from_anchor_toml(project_root: &PathBuf) -> Option<Pubkey> {
    let toml_path = project_root.join("Anchor.toml");
    let contents = std::fs::read_to_string(toml_path).ok()?;
    for line in contents.lines() {
        if let Some(eq_pos) = line.find('=') {
            let value = line[eq_pos + 1..].trim().trim_matches('"');
            if let Ok(pk) = value.parse() {
                return Some(pk);
            }
        }
    }
    None
}
