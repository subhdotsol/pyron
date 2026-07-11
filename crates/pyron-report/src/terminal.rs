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
    let pct_of_budget = (stats.p95 as f64 / BUDGET as f64) * 100.0;

    println!("  {}", stats.instruction.white().bold());

    let bar = make_bar(stats.p95, BUDGET);
    let cu_str = format_cu(stats.p95);
    let budget_str = format!("{:.1}% of budget", pct_of_budget);

    let colored_bar = if pct_of_budget > 80.0 {
        format!("  {} {} {}", bar.red(), cu_str.red(), budget_str.red())
    } else if pct_of_budget > 40.0 {
        format!("  {} {} {}", bar.yellow(), cu_str.yellow(), budget_str.dimmed())
    } else {
        format!("  {} {} {}", bar.green(), cu_str.green(), budget_str.dimmed())
    };
    println!("{}", colored_bar);

    println!(
        "  {} {}  {} {}  {} {}  {} {}",
        "p50".dimmed(),
        format_cu(stats.p50).cyan(),
        "p95".dimmed(),
        format_cu(stats.p95).cyan(),
        "max".dimmed(),
        format_cu(stats.max).cyan(),
        "runs".dimmed(),
        stats.runs.to_string().dimmed(),
    );

    println!(
        "  {} set_compute_unit_limit({})",
        "→".blue(),
        stats.recommended_cu_limit.to_string().blue().bold(),
    );

    if let Some(node) = tree {
        if !node.children.is_empty() {
            print_cpi_tree(node, 0, node.cu_consumed);
        }
    }

    println!();
}

pub fn print_cpi_tree(node: &CuNode, depth: usize, parent_cu: u64) {
    for child in &node.children {
        let indent = "  ".repeat(depth + 2);
        let connector = if depth == 0 { "└─" } else { "  └─" };
        let name = child
            .instruction
            .as_deref()
            .unwrap_or(&child.program[..child.program.len().min(16)]);
        let pct = if parent_cu > 0 {
            child.cu_consumed * 100 / parent_cu
        } else {
            0
        };
        println!(
            "{}{} {:<22} {}  ({}%)",
            indent,
            connector.dimmed(),
            name.dimmed(),
            format_cu(child.cu_consumed).dimmed(),
            pct.to_string().dimmed(),
        );
        print_cpi_tree(child, depth + 1, child.cu_consumed);
    }
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
