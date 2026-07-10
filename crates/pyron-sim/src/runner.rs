use crate::loader::IdlInstruction;
use anyhow::{anyhow, Context, Result};
use pyron_parser::{parse_logs, CuNode};
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSimulateTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
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
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://127.0.0.1:8899".to_string(),
            runs: 50,
            program_id: Pubkey::default(),
        }
    }
}

/// Simulate one instruction N times and return a CuNode for each run.
/// The caller aggregates these into p50/p95/max statistics.
pub fn simulate_instruction(config: &RunConfig, ix: &IdlInstruction) -> Result<Vec<CuNode>> {
    let client =
        RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());

    // Dummy keypair — we use sigVerify: false so it doesn't need to be real
    let payer = Keypair::new();

    // Build the instruction data: discriminator only (no args)
    // This is enough to trigger the instruction and see CU consumption
    // even if it fails due to missing accounts — we still get the logs
    let data = ix.discriminator.to_vec();

    let instruction = Instruction {
        program_id: config.program_id,
        accounts: Vec::new(), // empty for basic profiling
        data,
    };

    let sim_config = RpcSimulateTransactionConfig {
        // Don't verify signatures — we're using a dummy keypair
        sig_verify: false,
        // Replace blockhash automatically so tx is always fresh
        replace_recent_blockhash: true,
        // Get logs back
        commitment: Some(CommitmentConfig::confirmed()),
        ..Default::default()
    };

    let mut nodes = Vec::with_capacity(config.runs);

    for i in 0..config.runs {
        let message = Message::new(&[instruction.clone()], Some(&payer.pubkey()));
        let tx = Transaction::new_unsigned(message);

        let result = client
            .simulate_transaction_with_config(&tx, sim_config.clone())
            .with_context(|| format!("RPC simulate failed on run {}", i))?;

        let logs = result.value.logs.unwrap_or_default();

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
