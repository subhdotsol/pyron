# pyron

The first compute unit profiler for Solana programs.

pyron runs your instruction through `simulateTransaction`, parses the program logs into a structured CPI call tree, and renders an interactive flamegraph showing exactly where your compute budget goes down to which CPI call, which syscall category, and which part of your program logic. Includes a regression detector that blocks CI when a code change increases CU consumption by more than a configurable threshold.

---

## The Problem

Every Solana transaction has a hard cap of 1.4 million compute units. Devs today handle this by calling `ComputeBudget::setComputeUnitLimit` with a number they either guessed, copied from Stack Overflow, or got from one failed transaction on devnet. There is no tool that tells you *where* your CUs are going. It's like optimizing a slow Node.js app with no profiler you're just guessing.

## The Key Insight

Solana already logs CU data , nobody parses it.

When you call `simulateTransaction`, the response includes `logs`, and buried in those logs is exactly what you need:

```
Program vault invoke [1]
Program log: Instruction: Deposit
Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]
Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 4736 of 389420 compute units
Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success
Program vault consumed 45230 of 400000 compute units
```

The CPI tree is right there. Every `invoke [N]` is a depth level. Every `consumed X of Y` is a CU measurement. pyron turns these strings into a structured flamegraph.

---

## Features

- **CPI flamegraph** : interactive HTML report showing CU cost per program, per instruction, per CPI depth level
- **Multi-run profiling** : runs 10–50 simulations and reports p50/p95/max (CU consumption varies between simulations due to cache state)
- **Regression detector** : diffs against a saved baseline and exits non-zero when CU consumption grows beyond a configurable threshold
- **CI-ready** : `--max-regression 10` fails the build if any instruction regresses by more than 10%

---

## Usage

```bash
# Profile a specific instruction
pyron profile --program vault --ix deposit --runs 10

# Generate a flamegraph HTML report
pyron report --out cu-report.html

# Compare against a saved baseline
pyron diff --baseline cu-baseline.json --max-regression 10
```

### Example output

```
pyron profile --program vault --ix deposit

  vault::deposit          45,230 CU   100%
  ├─ spl-token::transfer   4,736 CU    10%
  ├─ pyth::get_price       8,100 CU    18%
  └─ vault (self)         32,394 CU    72%

  p50: 45,230   p95: 47,100   max: 49,800
```

### Regression detection

```bash
pyron diff --baseline cu-baseline.json

  deposit   45,230 → 53,100 CU  (+17.4%)  ⚠ REGRESSION
  withdraw  12,100 → 12,090 CU  (-0.1%)   ✓ stable

  exit 1
```

---

## Architecture

```
pyron/
├── pyron/                  # CLI entrypoint
├── crates/
│   ├── pyron-parser/       # Log parsing state machine → CuNode tree
│   ├── pyron-sim/          # simulateTransaction runner, multi-run aggregation
│   ├── pyron-report/       # Flamegraph HTML renderer (d3-flame-graph)
│   └── pyron-ai/           # AI-assisted optimization suggestions
```

### Log parser (`pyron-parser`)

The core of pyron. Takes `Vec<String>` of simulation logs and emits a typed tree:

```rust
pub struct CuNode {
    pub program: String,       // "vault", "spl-token", "pyth"
    pub instruction: String,   // "Deposit", "transfer"
    pub depth: usize,          // CPI depth — [1] = top level
    pub cu_consumed: u64,      // absolute CUs this program used
    pub cu_self: u64,          // CUs minus children (your logic only)
    pub children: Vec<CuNode>,
}
```

The state machine handles four log events:
- `Program X invoke [N]` → push new frame at depth N
- `Program log: Instruction: Y` → annotate current frame
- `Program X consumed N of M` → pop frame, record CU delta
- `Program X success/failed` → close frame

---

## CI Integration

Add to your GitHub Actions workflow:

```yaml
- name: Check CU regressions
  run: pyron diff --baseline cu-baseline.json --max-regression 10
```

Commit `cu-baseline.json` to your repo. Update it intentionally when you optimize — the diff makes every CU change visible in PR review.

---

## Build

```bash
cargo build --release
```

Requires Rust 1.75+.

---

## License

Apache-2.0 — see [LICENSE](LICENSE).
