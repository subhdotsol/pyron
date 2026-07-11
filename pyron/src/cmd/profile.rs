use anyhow::{Context, Result};
use pyron_parser::{aggregate, CuNode, CuStats};
use pyron_report::{
    clear_progress, generate_report, print_footer, print_header, print_instruction_stats,
    print_simulating,
};
use pyron_sim::{
    detect_idl, detect_program_id, fetch_tx_instructions, load_idl, resolve_payer_pubkey,
    simulate_from_tx, simulate_instruction, RunConfig,
};
use std::{path::PathBuf, time::Instant};

pub fn run(
    instruction_filter: Option<String>,
    runs: usize,
    rpc_url: String,
    program_id_override: Option<String>,
    idl_path_override: Option<String>,
    payer_override: Option<String>,
    tx_signature: Option<String>,
    report: bool,
    report_dir: String,
) -> Result<()> {
    let start = Instant::now();
    let cwd = std::env::current_dir()?;

    let program_id = match program_id_override {
        Some(ref id) => id.parse().context("Invalid program ID")?,
        None => detect_program_id(&cwd)
            .context("Could not detect program ID. Pass --program-id or run from your project root.")?,
    };

    print_header(&program_id.to_string(), &rpc_url, runs);

    let mut all_stats: Vec<CuStats> = Vec::new();
    let mut all_trees: Vec<Option<CuNode>> = Vec::new();
    let mut profiled = 0;

    // --tx mode: parse a real confirmed transaction and display all its instructions.
    // With --instruction: filter to just that one.
    // Without --instruction: show every program instruction in the tx.
    if let Some(ref sig) = tx_signature {
        match fetch_tx_instructions(&rpc_url, sig, &program_id, instruction_filter.as_deref()) {
            Ok(tx_nodes) => {
                for orig_node in tx_nodes {
                    let ix_name = orig_node
                        .instruction
                        .as_deref()
                        .unwrap_or("instruction")
                        .to_string();

                    // Use the original-tx node as run #1; then try to re-simulate
                    // for additional statistical runs (silently skips if state changed).
                    let mut stat_nodes = vec![orig_node.clone()];

                    if runs > 1 {
                        if let Ok(extra) = simulate_from_tx(
                            &rpc_url,
                            sig,
                            &program_id,
                            runs,
                            Some(&ix_name),
                        ) {
                            for n in extra {
                                if n.cu_consumed > 0 {
                                    stat_nodes.push(n);
                                }
                            }
                        }
                    }

                    let stats = aggregate(stat_nodes);
                    print_instruction_stats(&stats, Some(&orig_node));
                    all_stats.push(stats);
                    all_trees.push(Some(orig_node));
                    profiled += 1;
                }
            }
            Err(e) => eprintln!("  error: {}", e),
        }

        print_footer(profiled, start.elapsed().as_secs_f64());
        return Ok(());
    }

    // Normal IDL-based mode
    let idl_path = match idl_path_override {
        Some(ref p) => PathBuf::from(p),
        None => detect_idl(&cwd).context("No IDL found. Run `anchor build` first, or pass --idl")?,
    };

    let mut instructions = load_idl(&idl_path)
        .with_context(|| format!("Could not load IDL from {}", idl_path.display()))?;

    if let Some(ref name) = instruction_filter {
        instructions.retain(|ix| ix.name.to_lowercase().contains(&name.to_lowercase()));
    }

    if instructions.is_empty() {
        anyhow::bail!("No instructions match the filter. Check your IDL.");
    }

    let payer_pubkey = match payer_override {
        Some(ref s) => s.parse().context("Invalid payer pubkey")?,
        None => resolve_payer_pubkey(&rpc_url, &program_id)
            .context("Could not resolve fee payer. Pass --payer <PUBKEY>")?,
    };

    let config = RunConfig { rpc_url: rpc_url.clone(), runs, program_id, payer_pubkey };

    for ix in &instructions {
        for i in 0..runs {
            print_simulating(&ix.name, i, runs);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        match simulate_instruction(&config, ix) {
            Ok(nodes) => {
                clear_progress();
                let tree = nodes.first().cloned();
                let stats = aggregate(nodes);
                print_instruction_stats(&stats, tree.as_ref());
                all_stats.push(stats);
                all_trees.push(tree);
                profiled += 1;
            }
            Err(e) => {
                clear_progress();
                eprintln!("  error: {} {}", ix.name, e);
                all_trees.push(None);
            }
        }
    }

    print_footer(profiled, start.elapsed().as_secs_f64());

    if report {
        let pairs: Vec<_> = all_stats
            .iter()
            .zip(all_trees.iter())
            .map(|(s, t)| (s, t.as_ref()))
            .collect();

        let out_dir = PathBuf::from(&report_dir);

        match generate_report(&pairs, &program_id.to_string(), &out_dir) {
            Ok(path) => {
                println!("  report saved to {}", path);
                let _ = std::process::Command::new("open").arg(&path).spawn();
                let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
            }
            Err(e) => eprintln!("  report generation failed: {}", e),
        }
    }

    Ok(())
}
