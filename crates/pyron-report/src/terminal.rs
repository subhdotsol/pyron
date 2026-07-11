use colored::Colorize;
use pyron_parser::{CuNode, CuStats, DiffResult};

const BUDGET: u64 = 1_400_000;
const BAR_WIDTH: usize = 20;

pub fn print_header(program_id: &str, rpc_url: &str, runs: usize) {
    println!();
    println!("{} {}", "pyron".cyan().bold(), "— Solana Compute Unit Profiler".white());
    println!("  {} {}", "program:".dimmed(), program_id.dimmed());
    println!(
        "  {} {}",
        "rpc:    ".dimmed(),
        if rpc_url.len() > 40 {
            format!("{}...", &rpc_url[..40]).dimmed().to_string()
        } else {
            rpc_url.dimmed().to_string()
        }
    );
    println!(
        "  {} {} per instruction",
        "runs:   ".dimmed(),
        runs.to_string().dimmed()
    );
    println!();
    println!("{}", "─".repeat(52).dimmed());
    println!();
}

pub fn print_instruction_stats(stats: &CuStats, tree: Option<&CuNode>) {
    // Root line: program::instruction   45,230 CU   100%
    println!(
        "  {:<28} {}   {}",
        stats.instruction.white().bold(),
        format_cu(stats.p50).white().bold(),
        "100%".dimmed(),
    );

    if let Some(node) = tree {
        if !node.children.is_empty() {
            print_cpi_tree(node, node.cu_consumed);
        } else {
            print_no_cpi_breakdown(node);
        }
    }

    println!();
    println!(
        "  {} {}   {} {}   {} {}",
        "p50:".dimmed(),
        format_cu(stats.p50).cyan(),
        "p95:".dimmed(),
        format_cu(stats.p95).cyan(),
        "max:".dimmed(),
        format_cu(stats.max).cyan(),
    );
    println!(
        "  {} set_compute_unit_limit({})   {} runs",
        "→".blue(),
        stats.recommended_cu_limit.to_string().blue().bold(),
        stats.runs.to_string().dimmed(),
    );

    println!();
}

pub fn print_cpi_tree(node: &CuNode, parent_cu: u64) {
    let children = &node.children;
    let cu_children: u64 = children.iter().map(|c| c.cu_consumed).sum();
    let cu_self = node.cu_consumed.saturating_sub(cu_children);

    // Decide whether to show a (self) row: only when self cost is non-trivial (>1%)
    let show_self = cu_self > 0 && parent_cu > 0 && cu_self * 100 / parent_cu >= 1;
    let total_rows = children.len() + if show_self { 1 } else { 0 };

    for (i, child) in children.iter().enumerate() {
        let is_last = i + 1 == total_rows;
        let connector = if is_last { "└─" } else { "├─" };
        let name = match &child.instruction {
            Some(ix) => format!("{}::{}", shorten(&child.program), ix),
            None => shorten(&child.program).to_string(),
        };
        let pct = child.cu_consumed * 100 / parent_cu;
        println!(
            "  {} {:<28} {}   {}%",
            connector.dimmed(),
            name.dimmed(),
            format_cu(child.cu_consumed).dimmed(),
            pct.to_string().dimmed(),
        );
        if !child.children.is_empty() {
            print_cpi_subtree(child, child.cu_consumed, "  ");
        }
    }

    if show_self {
        let pct = cu_self * 100 / parent_cu;
        let self_label = format!("{} (self)", shorten(&node.program));
        println!(
            "  {} {:<28} {}   {}%",
            "└─".dimmed(),
            self_label.dimmed(),
            format_cu(cu_self).dimmed(),
            pct.to_string().dimmed(),
        );
    }
}

fn print_cpi_subtree(node: &CuNode, parent_cu: u64, prefix: &str) {
    let children = &node.children;
    for (i, child) in children.iter().enumerate() {
        let is_last = i + 1 == children.len();
        let connector = if is_last { "└─" } else { "├─" };
        let name = match &child.instruction {
            Some(ix) => format!("{}::{}", shorten(&child.program), ix),
            None => shorten(&child.program).to_string(),
        };
        let pct = if parent_cu > 0 { child.cu_consumed * 100 / parent_cu } else { 0 };
        println!(
            "  {}  {} {:<26} {}   {}%",
            prefix,
            connector.dimmed(),
            name.dimmed(),
            format_cu(child.cu_consumed).dimmed(),
            pct.to_string().dimmed(),
        );
        if !child.children.is_empty() {
            let new_prefix = format!("{}  ", prefix);
            print_cpi_subtree(child, child.cu_consumed, &new_prefix);
        }
    }
}

/// Called when an instruction has no CPI children.
/// If the program emitted sol_log_compute_units!() checkpoints, show a
/// labelled breakdown. If it only emitted plain msg!() logs, show those.
/// If it emitted nothing, show a (program logic) placeholder + tip.
fn print_no_cpi_breakdown(node: &pyron_parser::CuNode) {
    let checkpoints: Vec<_> = node.logs.iter()
        .filter(|l| l.cu_remaining.is_some())
        .collect();

    if !checkpoints.is_empty() {
        // Pair each checkpoint with the last plain log before it as the label.
        let mut segments: Vec<(String, u64)> = Vec::new();
        let mut pending_label = String::from("start");
        let mut prev_cu = node.budget_at_invocation;

        for entry in &node.logs {
            if let Some(cu_now) = entry.cu_remaining {
                let delta = prev_cu.saturating_sub(cu_now);
                if delta > 0 {
                    segments.push((pending_label.clone(), delta));
                }
                prev_cu = cu_now;
                pending_label = String::from("…");
            } else {
                pending_label = entry.message.clone();
            }
        }

        // Final segment from last checkpoint to end of instruction.
        let cu_after = node.budget_at_invocation.saturating_sub(node.cu_consumed);
        let final_delta = prev_cu.saturating_sub(cu_after);
        if final_delta > 0 {
            if pending_label == "…" {
                pending_label = String::from("cleanup");
            }
            segments.push((pending_label, final_delta));
        }

        let total = node.cu_consumed.max(1);
        for (i, (label, delta)) in segments.iter().enumerate() {
            let is_last = i + 1 == segments.len();
            let connector = if is_last { "└─" } else { "├─" };
            let pct = delta * 100 / total;
            println!(
                "  {} {:<28} {}   {}%",
                connector.dimmed(),
                label.dimmed(),
                format_cu(*delta).dimmed(),
                pct.to_string().dimmed(),
            );
        }
        return;
    }

    if !node.logs.is_empty() {
        // Plain msg!() logs but no CU checkpoints — show messages as annotations.
        for (i, entry) in node.logs.iter().enumerate() {
            let is_last = i + 1 == node.logs.len();
            let connector = if is_last { "└─" } else { "├─" };
            println!(
                "  {} {}",
                connector.dimmed(),
                entry.message.dimmed(),
            );
        }
        println!(
            "  {}",
            "    add sol_log_compute_units!() after each step for CU costs".dimmed()
        );
        return;
    }

    // No logs at all — show a placeholder.
    println!(
        "  {} {:<28} {}   100%",
        "└─".dimmed(),
        "(program logic — no CPI calls)".dimmed(),
        format_cu(node.cu_consumed).dimmed(),
    );
    println!(
        "  {}",
        "    add sol_log_compute_units!() in your program for a breakdown".dimmed()
    );
}

fn shorten(program: &str) -> &str {
    &program[..program.len().min(20)]
}

pub fn print_footer(count: usize, elapsed_secs: f64) {
    println!("{}", "─".repeat(52).dimmed());
    println!(
        "{} profiled {} instructions in {:.1}s",
        "✓".green().bold(),
        count,
        elapsed_secs,
    );
    println!();
}

pub fn print_simulating(name: &str, current: usize, total: usize) {
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let s = spinner[current % spinner.len()];
    print!(
        "\r  {} profiling {} [{}/{}]    ",
        s.cyan(),
        name.white(),
        current,
        total
    );
    use std::io::Write;
    std::io::stdout().flush().ok();
}

pub fn clear_progress() {
    print!("\r{}\r", " ".repeat(60));
    use std::io::Write;
    std::io::stdout().flush().ok();
}

pub fn print_diff_header(baseline_path: &str, saved_at: &str, threshold: f64) {
    println!();
    println!("{} {}", "pyron diff".cyan().bold(), "— regression check".white());
    println!("  {} {}", "baseline:".dimmed(), baseline_path.dimmed());
    println!("  {} {}", "saved at:".dimmed(), saved_at.dimmed());
    println!(
        "  {} {}% max regression allowed",
        "threshold:".dimmed(),
        threshold.to_string().dimmed()
    );
    println!();
    println!(
        "  {:<24} {:>12}  {:>12}  {:>8}  {}",
        "instruction".dimmed(),
        "baseline".dimmed(),
        "current".dimmed(),
        "delta".dimmed(),
        "status".dimmed(),
    );
    println!("  {}", "─".repeat(68).dimmed());
}

pub fn print_diff_row(result: &DiffResult) {
    let delta_str = if result.delta_pct > 0.0 {
        format!("+{:.1}%", result.delta_pct)
    } else {
        format!("{:.1}%", result.delta_pct)
    };

    let line = if result.is_regression {
        format!(
            "  {:<24} {:>12}  {:>12}  {:>8}  {}",
            result.instruction.red(),
            format_cu(result.baseline_cu).red(),
            format_cu(result.current_cu).red(),
            delta_str.red().bold(),
            "✗ REGRESSION".red().bold(),
        )
    } else if result.delta_pct > 5.0 {
        format!(
            "  {:<24} {:>12}  {:>12}  {:>8}  {}",
            result.instruction.yellow(),
            format_cu(result.baseline_cu).yellow(),
            format_cu(result.current_cu).yellow(),
            delta_str.yellow(),
            "⚠ within threshold".yellow(),
        )
    } else if result.delta_pct < 0.0 {
        format!(
            "  {:<24} {:>12}  {:>12}  {:>8}  {}",
            result.instruction.green(),
            format_cu(result.baseline_cu).green(),
            format_cu(result.current_cu).green(),
            delta_str.green(),
            "✓ improved".green(),
        )
    } else {
        format!(
            "  {:<24} {:>12}  {:>12}  {:>8}  {}",
            result.instruction.white(),
            format_cu(result.baseline_cu).dimmed(),
            format_cu(result.current_cu).dimmed(),
            delta_str.dimmed(),
            "✓".green(),
        )
    };

    println!("{}", line);
}

pub fn print_diff_summary(results: &[DiffResult]) -> i32 {
    let regressions: Vec<_> = results.iter().filter(|r| r.is_regression).collect();
    let improvements: Vec<_> = results.iter().filter(|r| r.delta_pct < 0.0).collect();

    println!();
    if regressions.is_empty() {
        println!("{} no regressions found", "✓".green().bold());
        if !improvements.is_empty() {
            println!("{} {} instruction(s) improved", "↓".green(), improvements.len());
        }
        println!();
        0
    } else {
        println!(
            "{} {} regression(s) found — exceeds threshold",
            "✗".red().bold(),
            regressions.len()
        );
        for r in &regressions {
            println!(
                "  {} {} +{:.1}% ({} → {} CU)",
                "→".red(),
                r.instruction.red(),
                r.delta_pct,
                r.baseline_cu,
                r.current_cu,
            );
        }
        println!();
        1
    }
}

fn make_bar(value: u64, max: u64) -> String {
    let filled = ((value as f64 / max as f64) * BAR_WIDTH as f64) as usize;
    let filled = filled.min(BAR_WIDTH);
    let empty = BAR_WIDTH - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

fn format_cu(cu: u64) -> String {
    let s = cu.to_string();
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(*c);
    }
    format!("{} CU", result)
}
