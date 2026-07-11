use crate::loader::IdlInstruction;
use anyhow::{anyhow, Context, Result};
use pyron_parser::{parse_logs, parse_logs_one, CuNode};
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSimulateTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    message::Message,
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use solana_transaction_status::{EncodedTransaction, UiMessage, UiTransactionEncoding};
use std::str::FromStr;

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

        // println!(
        //     "[run {}] logs ({} lines): {:?}",
        //     i,
        //     logs.len(),
        //     &logs[..logs.len().min(3)]
        // );
        // println!("[run {}] error: {:?}", i, result.value.err);

        if logs.is_empty() {
            // RPC returned no logs — probably wrong program ID or not deployed
            continue;
        }

        match parse_logs_one(&logs) {
            Ok(node) => nodes.push(node),
            Err(e) => eprintln!("[pyron] warning: parse failed on run {}: {}", i, e),
        }
    }

    if nodes.is_empty() {
        return Err(anyhow!(
            "No successful simulation runs. Is the validator running? Is the program deployed?",
        ));
    }

    Ok(nodes)
}

/// Resolve a usable fee payer pubkey for simulation by reading the program's
/// BPFLoaderUpgradeable data account and extracting the upgrade authority.
/// Falls back to None if the program is immutable or the account can't be read.
pub fn resolve_payer_pubkey(rpc_url: &str, program_id: &Pubkey) -> Option<Pubkey> {
    let client = RpcClient::new(rpc_url.to_string());

    let program_account = client.get_account(program_id).ok()?;
    if program_account.data.len() < 36 {
        return None;
    }
    // BPFLoaderUpgradeable Program account layout:
    //   [0..4]  u32 discriminant = 2
    //   [4..36] Pubkey = programdata_address
    let programdata_address = Pubkey::try_from(&program_account.data[4..36]).ok()?;

    let pd = client.get_account(&programdata_address).ok()?;
    if pd.data.len() < 45 || pd.data[12] == 0 {
        return None;
    }
    // BPFLoaderUpgradeable ProgramData account layout:
    //   [0..4]  u32 discriminant = 3
    //   [4..12] u64 slot
    //   [12]    u8  Option flag (1 = Some)
    //   [13..45] Pubkey = upgrade authority
    Pubkey::try_from(&pd.data[13..45]).ok()
}

/// Fetch a transaction and return ONE CuNode per program instruction it contains.
/// Filters out zero-CU native programs (ComputeBudget).
/// If `instruction_filter` is given, only returns instructions whose name matches.
pub fn fetch_tx_instructions(
    rpc_url: &str,
    tx_signature: &str,
    program_id: &Pubkey,
    instruction_filter: Option<&str>,
) -> Result<Vec<CuNode>> {
    let client =
        RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());
    let sig = Signature::from_str(tx_signature).context("Invalid transaction signature")?;
    let confirmed = client
        .get_transaction(&sig, UiTransactionEncoding::Json)
        .context("Could not fetch transaction — wrong signature or RPC URL?")?;

    let original_logs: Vec<String> = confirmed
        .transaction
        .meta
        .as_ref()
        .and_then(|m| m.log_messages.clone().into())
        .unwrap_or_default();

    if original_logs.is_empty() {
        anyhow::bail!("Transaction has no logs. Is it confirmed?");
    }

    let roots = parse_logs(&original_logs)
        .map_err(|e| anyhow!("Could not parse transaction logs: {}", e))?;

    // Keep only roots that belong to our program and have real CU cost
    let program_str = program_id.to_string();
    let mut matching: Vec<CuNode> = roots
        .into_iter()
        .filter(|n| n.cu_consumed > 0 && n.program.starts_with(&program_str[..8]))
        .collect();

    // Apply instruction name filter if given
    if let Some(filter) = instruction_filter {
        let filter_lc = filter.to_lowercase();
        matching.retain(|n| {
            n.instruction
                .as_deref()
                .map(|ix| ix.to_lowercase().contains(&filter_lc))
                .unwrap_or(false)
        });
    }

    if matching.is_empty() {
        anyhow::bail!(
            "No instructions for program {} found in that transaction{}",
            program_id,
            instruction_filter
                .map(|f| format!(" matching '{}'", f))
                .unwrap_or_default()
        );
    }

    Ok(matching)
}

/// Fetch a real confirmed transaction and profile it.
///
/// Primary source: parse the original transaction's logs directly — these
/// reflect the real CPI tree that ran, with correct state, no constraint
/// failures.  Then re-simulate up to `runs` times for statistical data
/// (p50/p95/max).  If re-simulation fails due to state drift (ConstraintSeeds,
/// AccountAlreadyInitialized, etc.), the original-log run is still returned so
/// the CPI tree is always shown.
pub fn simulate_from_tx(
    rpc_url: &str,
    tx_signature: &str,
    program_id: &Pubkey,
    runs: usize,
    instruction_filter: Option<&str>,
) -> Result<Vec<CuNode>> {
    let client =
        RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    let sig = Signature::from_str(tx_signature)
        .context("Invalid transaction signature")?;

    let confirmed = client
        .get_transaction(&sig, UiTransactionEncoding::Json)
        .context("Could not fetch transaction — wrong signature or RPC URL?")?;

    // ── 1. Parse the original transaction logs ────────────────────────────────
    // These are always correct: the tx executed with the right state, right
    // accounts, right PDAs.  Re-simulation may fail due to state drift, but
    // these logs never lie.
    let mut nodes: Vec<CuNode> = Vec::new();

    let original_logs: Vec<String> = confirmed
        .transaction
        .meta
        .as_ref()
        .and_then(|m| m.log_messages.clone().into())
        .unwrap_or_default();

    if !original_logs.is_empty() {
        match parse_logs(&original_logs) {
            Ok(roots) => {
                // Pick the root that matches the instruction filter (if given),
                // or the highest-CU root (most likely the instruction of interest).
                let picked = pick_root(roots, instruction_filter);
                if let Some(node) = picked {
                    nodes.push(node);
                }
            }
            Err(e) => eprintln!("[pyron] warning: could not parse original tx logs: {}", e),
        }
    }

    // ── 2. Re-simulate for additional statistical runs ────────────────────────
    // Re-simulation fails for instructions that create PDAs (they already exist)
    // or whose accounts changed state.  We try anyway and silently skip failures
    // so p50/p95/max are based on however many runs succeed.
    let ui_tx = match &confirmed.transaction.transaction {
        EncodedTransaction::Json(tx) => tx,
        _ => {
            // Can't re-simulate without JSON encoding — original log run is enough
            if nodes.is_empty() {
                anyhow::bail!("Could not parse transaction or its logs");
            }
            return Ok(nodes);
        }
    };

    let raw_msg = match &ui_tx.message {
        UiMessage::Raw(m) => m,
        _ => {
            if nodes.is_empty() {
                anyhow::bail!("Unexpected message format");
            }
            return Ok(nodes);
        }
    };

    let account_keys: Vec<Pubkey> = raw_msg
        .account_keys
        .iter()
        .map(|k| k.parse::<Pubkey>().context("Invalid pubkey in tx"))
        .collect::<Result<Vec<_>>>()?;

    let h = &raw_msg.header;
    let n_sigs = h.num_required_signatures as usize;
    let n_ro_signed = h.num_readonly_signed_accounts as usize;
    let n_ro_unsigned = h.num_readonly_unsigned_accounts as usize;
    let n_total = account_keys.len();
    let n_rw_signed = n_sigs.saturating_sub(n_ro_signed);

    let meta_for = |idx: usize| -> AccountMeta {
        let pk = account_keys[idx];
        let is_signer = idx < n_sigs;
        let is_writable = idx < n_rw_signed
            || (idx >= n_sigs && idx < n_total.saturating_sub(n_ro_unsigned));
        if is_writable {
            AccountMeta::new(pk, is_signer)
        } else {
            AccountMeta::new_readonly(pk, is_signer)
        }
    };

    let ui_ix = raw_msg.instructions.iter().find(|ix| {
        account_keys
            .get(ix.program_id_index as usize)
            .map(|pk| pk == program_id)
            .unwrap_or(false)
    });

    if let Some(ui_ix) = ui_ix {
        if let Ok(data) = bs58::decode(&ui_ix.data).into_vec() {
            let accounts: Vec<AccountMeta> = ui_ix
                .accounts
                .iter()
                .map(|&idx| meta_for(idx as usize))
                .collect();

            let instruction = Instruction {
                program_id: *program_id,
                accounts,
                data,
            };

            let payer_pubkey = account_keys[0];
            let sim_config = RpcSimulateTransactionConfig {
                sig_verify: false,
                replace_recent_blockhash: true,
                commitment: Some(CommitmentConfig::confirmed()),
                encoding: None,
                accounts: None,
                min_context_slot: None,
                inner_instructions: true,
            };

            // Run (runs - 1) additional simulations; the original log already
            // counts as run #1.
            let extra = runs.saturating_sub(1);
            for i in 0..extra {
                let message = Message::new(&[instruction.clone()], Some(&payer_pubkey));
                let tx = Transaction::new_unsigned(message);

                let result = client
                    .simulate_transaction_with_config(&tx, sim_config.clone())
                    .with_context(|| format!("RPC simulate failed on run {}", i + 1));

                let result = match result {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[pyron] warning: {}", e);
                        break;
                    }
                };

                let logs = result.value.logs.unwrap_or_default();
                if logs.is_empty() {
                    continue;
                }

                // Only keep runs that didn't error out — a Solana error in the
                // simulated tx means the instruction failed (state drift), so
                // those CU numbers reflect the abort path, not the success path.
                if result.value.err.is_some() {
                    continue;
                }

                match parse_logs(&logs) {
                    Ok(roots) => {
                        if let Some(node) = pick_root(roots, instruction_filter) {
                            nodes.push(node);
                        }
                    }
                    Err(e) => eprintln!("[pyron] warning: parse failed on run {}: {}", i + 1, e),
                }
            }
        }
    }

    if nodes.is_empty() {
        return Err(anyhow!(
            "Could not parse the transaction logs. Check the signature and RPC URL."
        ));
    }

    Ok(nodes)
}

/// From a list of root CuNodes (one per top-level program invocation in a tx),
/// pick the one matching the instruction filter. If no filter is given, return
/// the highest-CU root (skipping zero-CU bookkeeping programs like ComputeBudget).
fn pick_root(mut roots: Vec<CuNode>, filter: Option<&str>) -> Option<CuNode> {
    if let Some(name) = filter {
        let name_lc = name.to_lowercase();
        // Prefer exact instruction name match first
        if let Some(pos) = roots.iter().position(|n| {
            n.instruction
                .as_deref()
                .map(|ix| ix.to_lowercase().contains(&name_lc))
                .unwrap_or(false)
        }) {
            return Some(roots.remove(pos));
        }
    }
    // Fall back: highest CU (filters out zero-CU ComputeBudget nodes)
    roots.into_iter().max_by_key(|n| n.cu_consumed)
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
        println!(
            "recommended limit: {} CU (p95 + 10%)",
            stats.recommended_cu_limit
        );
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
