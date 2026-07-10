use colored::Colorize;
use pyron_parser::{CuNode, CuStats};

const BUDGET: u64 = 1_400_000;
const BAR_WIDTH: usize = 20;

pub fn print_header(program_id: &str, rpc_url: &str, runs: usize) {
    println!();
    println!("{} {}", "pyron".cyan().bold(), "Solana Compute Unit Profiler".white());
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
        "limit:".blue(),
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
        let connector = if depth == 0 { "-" } else { "  -" };
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
    println!(
        "done  profiled {} instructions in {:.1}s",
        count, elapsed_secs,
    );
    println!();
}

pub fn print_simulating(name: &str, current: usize, total: usize) {
    print!(
        "\r  profiling {} [{}/{}]    ",
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

fn make_bar(value: u64, max: u64) -> String {
    let filled = ((value as f64 / max as f64) * BAR_WIDTH as f64) as usize;
    let filled = filled.min(BAR_WIDTH);
    let empty = BAR_WIDTH - filled;
    format!("[{}{}]", "#".repeat(filled), ".".repeat(empty))
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
