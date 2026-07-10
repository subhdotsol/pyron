use crate::{error::ParseError, types::CuNode};

/// One open frame on the parser stack.
/// Created when we see "Program X invoke [N]".
/// Closed when we see "Program X consumed N of M".
#[derive(Debug)]
struct Frame {
    program: String,
    instruction: Option<String>,
    depth: usize,
    children: Vec<CuNode>,
}

/// Parse a slice of Solana simulation log lines into a CuNode tree.
///
/// # Example
/// ```
/// use pyron_parser::parse_logs;
/// let logs = vec![
///     "Program VAULT invoke [1]".to_string(),
///     "Program log: Instruction: Deposit".to_string(),
///     "Program VAULT consumed 45230 of 200000 compute units".to_string(),
///     "Program VAULT success".to_string(),
/// ];
/// let node = parse_logs(&logs).unwrap();
/// assert_eq!(node.cu_consumed, 45230);
/// ```
pub fn parse_logs(logs: &[String]) -> Result<CuNode, ParseError> {
    if logs.is_empty() {
        return Err(ParseError::EmptyLogs);
    }

    // Stack of open frames. We push on "invoke", pop on "consumed".
    let mut stack: Vec<Frame> = Vec::new();

    // The completed root node — set when the outermost frame closes.
    let mut root: Option<CuNode> = None;

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
                };

                if stack.is_empty() {
                    // This was the root frame — we're done
                    root = Some(node);
                } else {
                    // Add as child of the parent frame
                    stack.last_mut().unwrap().children.push(node);
                }
                let _ = budget_remaining; // used in extended version
                continue;
            }

            //  Pattern 4: "Program X success / failed"
            if rest.ends_with(" success") || rest.ends_with(" failed") {
                continue; // already handled by consumed line
            }
        }

        //  Pattern 2: "Program log: Instruction: X"
        if let Some(ix_name) = line.strip_prefix("Program log: Instruction: ") {
            if let Some(frame) = stack.last_mut() {
                frame.instruction = Some(ix_name.to_string());
            }
            continue;
        }

        // Any other log line (Program log: ..., Program data: ...) — skip
    }

    root.ok_or_else(|| ParseError::UnclosedFrame("root".to_string()))
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

        let node = parse_logs(&logs).expect("should parse");

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

        let node = parse_logs(&logs).expect("should parse");

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
    fn test_no_instruction_name() {
        let logs = vec![
            s("Program NativeProgram1111111111111111111 invoke [1]"),
            s("Program NativeProgram1111111111111111111 consumed 1200 of 200000 compute units"),
            s("Program NativeProgram1111111111111111111 success"),
        ];

        let node = parse_logs(&logs).expect("should parse");
        assert_eq!(node.cu_consumed, 1200);
        assert_eq!(node.instruction, None);
    }
}
