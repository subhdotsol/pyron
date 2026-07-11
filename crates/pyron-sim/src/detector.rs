use anyhow::{Context, Result};
use solana_sdk::pubkey::Pubkey;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum Framework {
    Anchor,
    Native,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub framework: Framework,
    pub so_path: PathBuf,
    pub idl_path: Option<PathBuf>,
    pub program_name: String,
}

pub fn detect(project_root: &Path) -> Result<ProjectInfo> {
    let is_anchor = project_root.join("Anchor.toml").exists();

    let deploy_dir = project_root.join("target/deploy");
    let so_path = find_so_file(&deploy_dir).context(
        "Could not find a compiled .so in target/deploy/. Run `anchor build` first.",
    )?;

    let program_name = so_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let idl_path = if is_anchor {
        find_idl_file(&project_root.join("target/idl"), &program_name)
    } else {
        None
    };

    Ok(ProjectInfo {
        framework: if is_anchor { Framework::Anchor } else { Framework::Native },
        so_path,
        idl_path,
        program_name,
    })
}

/// Detect IDL without requiring a compiled .so.
/// Searches target/idl/ for any .json file.
pub fn detect_idl(project_root: &Path) -> Option<PathBuf> {
    let idl_dir = project_root.join("target/idl");

    // try to name-match using Anchor.toml program name
    if let Some(name) = detect_program_name(project_root) {
        let exact = idl_dir.join(format!("{}.json", name));
        if exact.exists() {
            return Some(exact);
        }
    }

    // fallback: first .json in target/idl/
    let pattern = idl_dir.join("*.json").to_string_lossy().to_string();
    glob::glob(&pattern).ok()?.filter_map(|e| e.ok()).next()
}

/// Detect program ID from (in priority order):
/// 1. Anchor.toml [programs.*] section
/// 2. declare_id!("...") in programs/*/src/lib.rs or src/lib.rs
pub fn detect_program_id(project_root: &Path) -> Option<Pubkey> {
    if let Some(pk) = read_from_anchor_toml(project_root) {
        return Some(pk);
    }
    grep_declare_id(project_root)
}

fn detect_program_name(project_root: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(project_root.join("Anchor.toml")).ok()?;
    // look for lines like: my_program = "PubkeyXxx..."
    for line in contents.lines() {
        if let Some(eq) = line.find('=') {
            let value = line[eq + 1..].trim().trim_matches('"');
            // if it looks like a pubkey skip — we want the key (program name)
            if value.len() > 20 && value.parse::<Pubkey>().is_ok() {
                let name = line[..eq].trim().to_string();
                return Some(name);
            }
        }
    }
    None
}

fn read_from_anchor_toml(root: &Path) -> Option<Pubkey> {
    let contents = std::fs::read_to_string(root.join("Anchor.toml")).ok()?;
    for line in contents.lines() {
        if let Some(eq) = line.find('=') {
            let value = line[eq + 1..].trim().trim_matches('"');
            if let Ok(pk) = value.parse::<Pubkey>() {
                return Some(pk);
            }
        }
    }
    None
}

fn grep_declare_id(root: &Path) -> Option<Pubkey> {
    // Anchor: programs/*/src/lib.rs
    // Native/Pinocchio: src/lib.rs or src/processor.rs
    let candidates = [
        "programs/*/src/lib.rs",
        "programs/*/src/*.rs",
        "src/lib.rs",
        "src/processor.rs",
        "src/entrypoint.rs",
    ];

    for pattern in &candidates {
        let full = root.join(pattern).to_string_lossy().to_string();
        if let Ok(paths) = glob::glob(&full) {
            for path in paths.filter_map(|p| p.ok()) {
                if let Ok(src) = std::fs::read_to_string(&path) {
                    if let Some(pk) = extract_declare_id(&src) {
                        return Some(pk);
                    }
                }
            }
        }
    }
    None
}

fn extract_declare_id(source: &str) -> Option<Pubkey> {
    for line in source.lines() {
        let t = line.trim();
        // matches: declare_id!("...") and solana_program::declare_id!("...")
        if t.contains("declare_id!") {
            if let Some(start) = t.find('"') {
                if let Some(end) = t[start + 1..].find('"') {
                    let pk_str = &t[start + 1..start + 1 + end];
                    if let Ok(pk) = pk_str.parse::<Pubkey>() {
                        return Some(pk);
                    }
                }
            }
        }
    }
    None
}

fn find_so_file(deploy_dir: &Path) -> Option<PathBuf> {
    let pattern = deploy_dir.join("*.so").to_string_lossy().to_string();
    glob::glob(&pattern)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|p| !p.to_string_lossy().contains("keypair"))
}

fn find_idl_file(idl_dir: &Path, program_name: &str) -> Option<PathBuf> {
    let exact = idl_dir.join(format!("{}.json", program_name));
    if exact.exists() {
        return Some(exact);
    }
    let pattern = idl_dir.join("*.json").to_string_lossy().to_string();
    glob::glob(&pattern).ok()?.filter_map(|e| e.ok()).next()
}
