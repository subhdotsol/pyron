use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

/// One instruction from the IDL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlInstruction {
    pub name: String,
    /// 8-byte Anchor discriminator for this instruction
    pub discriminator: [u8; 8],
}

/// Minimal IDL structure we need — Anchor IDLs have more fields
/// but we only care about instruction names right now
#[derive(Debug, Deserialize)]
struct RawIdl {
    instructions: Vec<RawInstruction>,
}

#[derive(Debug, Deserialize)]
struct RawInstruction {
    name: String,
}

/// Load an Anchor IDL JSON file and return a list of instructions
/// with their computed discriminators.
pub fn load_idl(idl_path: &Path) -> Result<Vec<IdlInstruction>> {
    let contents = fs::read_to_string(idl_path)
        .with_context(|| format!("Could not read IDL at {}", idl_path.display()))?;

    let raw: RawIdl = serde_json::from_str(&contents)
        .context("IDL JSON is not valid — run `anchor build` to regenerate it")?;

    let instructions = raw
        .instructions
        .into_iter()
        .map(|ix| IdlInstruction {
            discriminator: anchor_discriminator(&ix.name),
            name: ix.name,
        })
        .collect();

    Ok(instructions)
}

/// Compute the 8-byte Anchor discriminator for an instruction name.
/// Formula: sha256("global:{name}")[0..8]
pub fn anchor_discriminator(name: &str) -> [u8; 8] {
    let preimage = format!("global:{}", name);
    let hash = Sha256::digest(preimage.as_bytes());
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash[..8]);
    disc
}

/// Given a discriminator from a transaction, find the instruction name.
/// Returns None if the discriminator doesn't match any known instruction.
pub fn resolve_instruction(
    discriminator: &[u8],
    instructions: &[IdlInstruction],
) -> Option<String> {
    if discriminator.len() < 8 {
        return None;
    }
    let disc: [u8; 8] = discriminator[..8].try_into().ok()?;
    instructions
        .iter()
        .find(|ix| ix.discriminator == disc)
        .map(|ix| ix.name.clone())
}
