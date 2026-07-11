use crate::{error::ParseError, types::{CuNode, LogEntry}};

/// One open frame on the parser stack.
/// Created when we see "Program X invoke [N]".
/// Closed when we see "Program X consumed N of M".
#[derive(Debug)]
struct Frame {
    program: String,
    instruction: Option<String>,
    depth: usize,
    children: Vec<CuNode>,
    logs: Vec<LogEntry>,
}

/// Parse a slice of Solana simulation log lines into a CuNode tree.
///
/// # Example
/// ```
/// use pyron_parser::parse_logs_one;
/// let logs = vec![
///     "Program VAULT invoke [1]".to_string(),
///     "Program log: Instruction: Deposit".to_string(),
///     "Program VAULT consumed 45230 of 200000 compute units".to_string(),
///     "Program VAULT success".to_string(),
/// ];
/// let node = parse_logs_one(&logs).unwrap();
/// assert_eq!(node.cu_consumed, 45230);
/// ```
/// Parse simulation logs and return **all** root-level program invocations.
/// A transaction typically has [ComputeBudget, program_ix_1, program_ix_2, …],
/// each of which closes at depth 1 and becomes its own root node.
/// The caller picks the one it cares about (e.g. by instruction name).
///
/// The single-root convenience wrapper [`parse_logs_one`] returns the highest-CU root.
pub fn parse_logs(logs: &[String]) -> Result<Vec<CuNode>, ParseError> {
    if logs.is_empty() {
        return Err(ParseError::EmptyLogs);
    }

    // Stack of open frames. We push on "invoke", pop on "consumed".
    let mut stack: Vec<Frame> = Vec::new();

    // All closed root-level frames, in log order.
    let mut roots: Vec<CuNode> = Vec::new();

    for line in logs {
        let line = line.trim();

        // Pattern 1: "Program X invoke [N]"
        if let Some(rest) = line.strip_prefix("Program ") {
            if let Some(idx) = rest.find(" invoke [") {
                let program = rest[..idx].to_string();
                let depth_str = &rest[idx + " invoke [".len()..];
                let depth: usize = depth_str
                    .trim_end_matches(']')
                    .parse()
                    .map_err(|_| ParseError::MalformedLog(line.to_string()))?;

                // We don't know the budget at entry yet —
                // we'll learn it from the "consumed N of M" line (M = budget remaining).
                // For now push with 0 — we'll fill it in when the child closes.
                stack.push(Frame {
                    program,
                    instruction: None,
                    depth,
                    children: Vec::new(),
                    logs: Vec::new(),
                });
                continue;
            }

            //  Pattern 3: "Program X consumed N of M compute units"
            if let Some(idx) = rest.find(" consumed ") {
                let _program = &rest[..idx]; // we trust the stack order
                let amounts = &rest[idx + " consumed ".len()..];
                // amounts = "45230 of 200000 compute units"
                let parts: Vec<&str> = amounts.splitn(3, ' ').collect();
                if parts.len() < 3 {
                    return Err(ParseError::MalformedLog(line.to_string()));
                }
                let cu_used: u64 = parts[0]
                    .parse()
                    .map_err(|_| ParseError::MalformedLog(line.to_string()))?;
                let budget_remaining: u64 = parts[2]
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse()
                    .map_err(|_| ParseError::MalformedLog(line.to_string()))?;

                // Pop the matching frame
                let frame = stack.pop().ok_or(ParseError::UnexpectedConsumed)?;

                // cu_consumed is simply cu_used from the log line
                let cu_consumed = cu_used;

                // cu_self = total - sum of children's costs
                let children_cu: u64 = frame.children.iter().map(|c| c.cu_consumed).sum();
                let cu_self = cu_consumed.saturating_sub(children_cu);

                let node = CuNode {
                    program: frame.program,
                    instruction: frame.instruction,
                    depth: frame.depth,
                    cu_consumed,
                    cu_self,
                    children: frame.children,
                    success: true, // set to false if "failed" line follows
                    logs: frame.logs,
                    budget_at_invocation: budget_remaining,
                };

                if stack.is_empty() {
                    roots.push(node);
                } else {
                    stack.last_mut().unwrap().children.push(node);
                }
                let _ = budget_remaining; // used in extended version
                continue;
            }

            //  Pattern 4: "Program X success / failed"
            //  Normally the "consumed" line already popped the frame, so this
            //  is a no-op.  Exception: native programs like ComputeBudget emit
            //  `invoke [N]` then `success` with NO `consumed` line.  In that
            //  case the frame is still on the stack — pop it now with cu = 0.
            if rest.ends_with(" success") || rest.ends_with(" failed") {
                let success = rest.ends_with(" success");
                let prog_name = if success {
                    rest.strip_suffix(" success").unwrap_or("")
                } else {
                    rest.strip_suffix(" failed").unwrap_or("")
                };
                if stack.last().map(|f| f.program.as_str()) == Some(prog_name) {
                    let frame = stack.pop().unwrap();
                    let node = CuNode {
                        program: frame.program,
                        instruction: frame.instruction,
                        depth: frame.depth,
                        cu_consumed: 0,
                        cu_self: 0,
                        children: frame.children,
                        success,
                        logs: frame.logs,
                        budget_at_invocation: 0,
                    };
                    if stack.is_empty() {
                        roots.push(node);
                    } else {
                        stack.last_mut().unwrap().children.push(node);
                    }
                }
                continue;
            }
        }

        //  Pattern 2: "Program log: ..."
        if let Some(log_msg) = line.strip_prefix("Program log: ") {
            if let Some(ix_name) = log_msg.strip_prefix("Instruction: ") {
                if let Some(frame) = stack.last_mut() {
                    frame.instruction = Some(ix_name.to_string());
                }
            } else if let Some(frame) = stack.last_mut() {
                let cu_remaining = parse_cu_remaining(log_msg);
                frame.logs.push(LogEntry {
                    message: log_msg.to_string(),
                    cu_remaining,
                });
            }
            continue;
        }

        // Any other log line (Program data: ...) — skip
    }

    if roots.is_empty() {
        Err(ParseError::UnclosedFrame("root".to_string()))
    } else {
        Ok(roots)
    }
}

/// Convenience: return the single highest-CU root from a simulation run.
/// For single-instruction simulations this is always the only result.
pub fn parse_logs_one(logs: &[String]) -> Result<CuNode, ParseError> {
    let mut roots = parse_logs(logs)?;
    roots.sort_by_key(|n| n.cu_consumed);
    roots.pop().ok_or(ParseError::EmptyLogs)
}

/// Parse "X compute units remaining" → Some(X), or None for any other log.
fn parse_cu_remaining(msg: &str) -> Option<u64> {
    let rest = msg.strip_suffix(" compute units remaining")?;
    rest.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn test_single_instruction_no_cpi() {
        let logs = vec![
            s("Program VAULT1111111111111111111111111111 invoke [1]"),
            s("Program log: Instruction: Deposit"),
            s("Program VAULT1111111111111111111111111111 consumed 45230 of 200000 compute units"),
            s("Program VAULT1111111111111111111111111111 success"),
        ];

        let node = parse_logs_one(&logs).expect("should parse");
        assert_eq!(node.cu_consumed, 45230);
        assert_eq!(node.cu_self, 45230);
        assert_eq!(node.depth, 1);
        assert_eq!(node.instruction.as_deref(), Some("Deposit"));
        assert!(node.children.is_empty());
    }

    #[test]
    fn test_with_one_cpi() {
        let logs = vec![
            s("Program VAULT1111111111111111111111111111 invoke [1]"),
            s("Program log: Instruction: Deposit"),
            s("Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]"),
            s("Program log: Instruction: Transfer"),
            s("Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 4736 of 194770 compute units"),
            s("Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success"),
            s("Program VAULT1111111111111111111111111111 consumed 45230 of 200000 compute units"),
            s("Program VAULT1111111111111111111111111111 success"),
        ];

        let node = parse_logs_one(&logs).expect("should parse");
        assert_eq!(node.cu_consumed, 45230);
        assert_eq!(node.children.len(), 1);

        let child = &node.children[0];
        assert_eq!(child.cu_consumed, 4736);
        assert_eq!(child.depth, 2);
        assert_eq!(child.instruction.as_deref(), Some("Transfer"));
        assert_eq!(node.cu_self, 45230 - 4736);
    }

    #[test]
    fn test_empty_logs_returns_error() {
        let result = parse_logs(&[]);
        assert!(matches!(result, Err(ParseError::EmptyLogs)));
    }

    #[test]
    fn test_compute_budget_before_main_program() {
        let logs = vec![
            s("Program ComputeBudget111111111111111111111111111111 invoke [1]"),
            s("Program ComputeBudget111111111111111111111111111111 success"),
            s("Program VAULT1111111111111111111111111111 invoke [1]"),
            s("Program log: Instruction: PlacePrivateBetYesno"),
            s("Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]"),
            s("Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 4736 of 180000 compute units"),
            s("Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success"),
            s("Program VAULT1111111111111111111111111111 consumed 45230 of 200000 compute units"),
            s("Program VAULT1111111111111111111111111111 success"),
        ];

        // parse_logs returns both ComputeBudget (cu=0) and VAULT (cu=45230)
        let roots = parse_logs(&logs).expect("should parse");
        assert_eq!(roots.len(), 2);

        // parse_logs_one picks the highest-CU root
        let node = parse_logs_one(&logs).expect("should parse");
        assert_eq!(node.program, "VAULT1111111111111111111111111111");
        assert_eq!(node.cu_consumed, 45230);
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].cu_consumed, 4736);
    }

    #[test]
    fn test_two_instructions_same_program() {
        // Transaction with two instructions from the same program (like the user's
        // place_private_bet_yesno + submit_bet_yesno in one tx)
        let logs = vec![
            s("Program ComputeBudget111111111111111111111111111111 invoke [1]"),
            s("Program ComputeBudget111111111111111111111111111111 success"),
            s("Program VAULT1111111111111111111111111111 invoke [1]"),
            s("Program log: Instruction: PlacePrivateBetYesno"),
            s("Program VAULT1111111111111111111111111111 consumed 30000 of 200000 compute units"),
            s("Program VAULT1111111111111111111111111111 success"),
            s("Program VAULT1111111111111111111111111111 invoke [1]"),
            s("Program log: Instruction: SubmitBetYesno"),
            s("Program VAULT1111111111111111111111111111 consumed 146235 of 170000 compute units"),
            s("Program VAULT1111111111111111111111111111 success"),
        ];

        let roots = parse_logs(&logs).expect("should parse");
        // ComputeBudget + PlacePrivateBetYesno + SubmitBetYesno = 3 roots
        assert_eq!(roots.len(), 3);

        // pick_root with filter finds PlacePrivateBetYesno
        let place = roots.iter().find(|n| {
            n.instruction.as_deref() == Some("PlacePrivateBetYesno")
        }).expect("should find PlacePrivateBetYesno");
        assert_eq!(place.cu_consumed, 30000);

        // pick_root with filter finds SubmitBetYesno
        let submit = roots.iter().find(|n| {
            n.instruction.as_deref() == Some("SubmitBetYesno")
        }).expect("should find SubmitBetYesno");
        assert_eq!(submit.cu_consumed, 146235);
    }

    #[test]
    fn test_no_instruction_name() {
        let logs = vec![
            s("Program NativeProgram1111111111111111111 invoke [1]"),
            s("Program NativeProgram1111111111111111111 consumed 1200 of 200000 compute units"),
            s("Program NativeProgram1111111111111111111 success"),
        ];

        let node = parse_logs_one(&logs).expect("should parse");
        assert_eq!(node.cu_consumed, 1200);
        assert_eq!(node.instruction, None);
    }
}
