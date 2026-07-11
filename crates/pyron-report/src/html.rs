use pyron_parser::{CuNode, CuStats};
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Serialize)]
struct FlameNode {
    name: String,
    value: u64,
    children: Vec<FlameNode>,
}

fn to_flame(node: &CuNode) -> FlameNode {
    let name = match &node.instruction {
        Some(ix) => format!("{} ({})", ix, &node.program[..node.program.len().min(8)]),
        None => node.program[..node.program.len().min(16)].to_string(),
    };
    FlameNode {
        name,
        value: node.cu_self,
        children: node.children.iter().map(to_flame).collect(),
    }
}

pub fn generate_report(
    results: &[(&CuStats, Option<&CuNode>)],
    program_id: &str,
    output_dir: &Path,
) -> Result<String, String> {
    fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

    let root = FlameNode {
        name: format!("program: {}", &program_id[..program_id.len().min(16)]),
        value: 0,
        children: results
            .iter()
            .filter_map(|(stats, tree)| {
                tree.map(|node| {
                    let mut flame = to_flame(node);
                    flame.name = format!("{} — {} CU", stats.instruction, stats.p95);
                    flame
                })
            })
            .collect(),
    };

    let flame_json = serde_json::to_string(&root).map_err(|e| e.to_string())?;

    let table_rows: String = results
        .iter()
        .map(|(stats, _)| {
            let pct = (stats.p95 as f64 / 1_400_000.0) * 100.0;
            let color = if pct > 80.0 {
                "#f85149"
            } else if pct > 40.0 {
                "#e3b341"
            } else {
                "#56d364"
            };
            format!(
                "<tr><td>{}</td><td style='color:{}'>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                stats.instruction,
                color,
                stats.p95,
                stats.p50,
                stats.max,
                stats.recommended_cu_limit,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>pyron — Compute Unit Report</title>
<script src="https://cdn.jsdelivr.net/npm/d3@7"></script>
<script src="https://cdn.jsdelivr.net/npm/d3-flame-graph@4/dist/d3-flamegraph.min.js"></script>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/d3-flame-graph@4/dist/d3-flamegraph.css">
<style>
  body {{ font-family: monospace; background: #0d1117; color: #c9d1d9; padding: 2rem; }}
  h1 {{ color: #58a6ff; }} h2 {{ color: #8b949e; font-size: 0.9rem; margin-top: 2rem; }}
  table {{ border-collapse: collapse; width: 100%; margin-top: 1rem; }}
  th, td {{ padding: 0.4rem 0.8rem; text-align: left; border-bottom: 1px solid #21262d; }}
  th {{ color: #8b949e; font-size: 0.8rem; }}
  #flamegraph {{ margin-top: 1.5rem; }}
  .tip {{ color: #8b949e; font-size: 0.8rem; margin-top: 1.5rem; }}
</style>
</head>
<body>
<h1>pyron</h1>
<p>program: <code>{program_id}</code></p>

<h2>flamegraph — click to zoom, hover to inspect</h2>
<div id="flamegraph"></div>

<h2>instruction stats</h2>
<table>
  <thead><tr><th>instruction</th><th>p95 CU</th><th>p50 CU</th><th>max CU</th><th>recommended limit</th></tr></thead>
  <tbody>{table_rows}</tbody>
</table>

<p class="tip">recommended limit = p95 + 10% safety buffer — paste into ComputeBudgetProgram::set_compute_unit_limit()</p>

<script>
  const data = {flame_json};
  const chart = flamegraph().width(document.body.clientWidth - 64).cellHeight(20);
  d3.select("#flamegraph").datum(data).call(chart);
</script>
</body>
</html>"##,
        program_id = program_id,
        table_rows = table_rows,
        flame_json = flame_json,
    );

    let out_path = output_dir.join("index.html");
    fs::write(&out_path, html).map_err(|e| e.to_string())?;

    Ok(out_path.to_string_lossy().to_string())
}
