use crate::loader::IdlInstruction;
use anyhow::{anyhow, Context, Result};
use pyron_parser::{parse_logs, CuNode};
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSimulateTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig, instruction::Instruction, message::Message,
    pubkey::Pubkey, transaction::Transaction,
};

/// Configuration for a single profiling run
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// RPC endpoint — defaults to localhost:8899
    pub rpc_url: String,
    /// How many times to simulate each instruction
    pub runs: usize,
    /// The program to profile
    pub program_id: Pubkey,
    /// An existing on-chain account to use as the fee payer.
    /// We never sign with it (sig_verify: false), but Solana's simulation
    /// still loads the payer account to check lamports — so it must exist.
    pub payer_pubkey: Pubkey,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://127.0.0.1:8899".to_string(),
            runs: 50,
            program_id: Pubkey::default(),
            payer_pubkey: Pubkey::default(),
        }
    }
}

/// Simulate one instruction N times and return a CuNode for each run.
/// The caller aggregates these into p50/p95/max statistics.
pub fn simulate_instruction(config: &RunConfig, ix: &IdlInstruction) -> Result<Vec<CuNode>> {
    let client =
        RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());

    let data = ix.discriminator.to_vec();

    let instruction = Instruction {
        program_id: config.program_id,
        accounts: Vec::new(),
        data,
    };

    let sim_config = RpcSimulateTransactionConfig {
        // Don't verify signatures — we're using a dummy keypair
        sig_verify: false,
        // Replace blockhash automatically so tx is always fresh
        replace_recent_blockhash: true,
        // Get logs back
        commitment: Some(CommitmentConfig::confirmed()),

        encoding: None,
        accounts: None,
        min_context_slot: None,
        inner_instructions: true,
    };

    let mut nodes = Vec::with_capacity(config.runs);

    for i in 0..config.runs {
        let message = Message::new(&[instruction.clone()], Some(&config.payer_pubkey));
        let tx = Transaction::new_unsigned(message);

        let result = client
            .simulate_transaction_with_config(&tx, sim_config.clone())
            .with_context(|| format!("RPC simulate failed on run {}", i))?;

        let logs = result.value.logs.unwrap_or_default();

        println!(
            "[run {}] logs ({} lines): {:?}",
            i,
            logs.len(),
            &logs[..logs.len().min(3)]
        );
        println!("[run {}] error: {:?}", i, result.value.err);

        if logs.is_empty() {
            // RPC returned no logs — probably wrong program ID or not deployed
            continue;
        }

        // Parse the logs into a CuNode tree
        match parse_logs(&logs) {
            Ok(node) => nodes.push(node),
            Err(e) => {
                // Log the error but don't abort — other runs may succeed
                eprintln!("[pyron] warning: parse failed on run {}: {}", i, e);
            }
        }
    }

    if nodes.is_empty() {
        return Err(anyhow!(
            "No successful simulation runs. Is the validator running? Is the program deployed?",
        ));
    }

    Ok(nodes)
}

/// Simulate ALL instructions in the IDL and return results per instruction.
pub fn simulate_all(
    config: &RunConfig,
    instructions: &[IdlInstruction],
) -> Vec<(String, Result<Vec<CuNode>>)> {
    instructions
        .iter()
        .map(|ix| {
            let result = simulate_instruction(config, ix);
            (ix.name.clone(), result)
        })
        .collect()
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{anchor_discriminator, load_idl};
    use pyron_parser::aggregate;
    use std::path::PathBuf;

    #[test]
    #[ignore]
    fn test_devnet_profile() {
        // 1. YOUR PROGRAM ID
        let program_id: Pubkey = "6tAck77wPs2S9ZWUF5vP71FosR6YSkTvBNzS5ZLHM3YP"
            .parse()
            .expect("paste your devnet program ID above");

        //  2. YOUR IDL PATH
        // adjust this to point at your anchor project's IDL
        let idl_path = PathBuf::from("../../multisig_vault.json");

        //  3. DEVNET RPC
        let rpc_url = "https://devnet.helius-rpc.com/?api-key=8c79234f-3452-457b-96e3-171b70c0cfd4"
            .to_string();
        // or use helius: "https://devnet.helius-rpc.com/?api-key=YOUR_KEY"

        // load IDL
        let instructions = if idl_path.exists() {
            load_idl(&idl_path).expect("IDL should load")
        } else {
            // fallback: manually define instructions if you know their names
            println!("IDL not found at {:?}, using manual definition", idl_path);
            vec![IdlInstruction {
                name: "initialize".to_string(),
                discriminator: anchor_discriminator("initialize"),
            }]
        };

        println!("found {} instructions in IDL:", instructions.len());
        for ix in &instructions {
            println!("  {} {:?}", ix.name, ix.discriminator);
        }

        // run simulation
        // payer_pubkey must be an existing on-chain account (sig_verify: false,
        // so we don't need the private key — but the account must exist so
        // Solana's simulation can load it for the fee check)
        let payer_pubkey: Pubkey = "5GHnVhqZ6Yn8mQmM43CwMRTNWZofeyExMkP6PDGCAc9d"
            .parse()
            .unwrap();

        let config = RunConfig {
            rpc_url,
            runs: 10,
            program_id,
            payer_pubkey,
        };

        // profile initialize_vault — it only needs a signer + system_program,
        // so it runs without pre-existing PDAs (safe first instruction to profile)
        let ix = instructions
            .iter()
            .find(|ix| ix.name == "initialize_vault")
            .unwrap_or(&instructions[0]);
        println!("profiling {} ({} runs)", ix.name, config.runs);

        let nodes = simulate_instruction(&config, ix)
            .expect("simulation failed — check program ID and RPC URL");

        println!("{} successful simulation runs", nodes.len());

        // print raw results
        for (i, node) in nodes.iter().enumerate() {
            println!(
                "  run {:2}: {:6} CU   instruction={:?}   children={}",
                i + 1,
                node.cu_consumed,
                node.instruction,
                node.children.len()
            );
        }

        //  aggregate stats
        let stats = aggregate(nodes.clone());

        println!("stats for {}:", stats.instruction);
        println!("  min: {} CU", stats.min);
        println!("  p50: {} CU", stats.p50);
        println!("  p95: {} CU", stats.p95);
        println!("  max: {} CU", stats.max);
        println!("  runs: {}", stats.runs);
        println!("recommended limit: {} CU (p95 + 10%)", stats.recommended_cu_limit);
        println!(
            "ComputeBudgetProgram::set_compute_unit_limit({});",
            stats.recommended_cu_limit
        );

        if let Some(first) = nodes.first() {
            println!("CPI tree (run 1):");
            print_tree(first, 0, first.cu_consumed);
        }

        // assertions
        assert!(!nodes.is_empty(), "should have at least one run");
        assert!(stats.p50 > 0, "p50 should be non-zero");
    }

    /// Recursively print a CuNode tree to the terminal
    fn print_tree(node: &pyron_parser::CuNode, indent: usize, parent_cu: u64) {
        let prefix = if indent == 0 {
            "  ".to_string()
        } else {
            "  ".repeat(indent) + "- "
        };

        let name = node
            .instruction
            .as_deref()
            .unwrap_or(&node.program[..node.program.len().min(12)]);

        let pct = if parent_cu > 0 {
            node.cu_consumed * 100 / parent_cu
        } else {
            100
        };

        println!(
            "{}{:<20} {:6} CU   {}%",
            prefix, name, node.cu_consumed, pct
        );

        for child in &node.children {
            print_tree(child, indent + 1, node.cu_consumed);
        }
    }
}
