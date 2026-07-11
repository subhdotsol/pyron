pub mod aggregator;
pub mod baseline;
pub mod error;
pub mod parser;
pub mod types;

// Re-export the most-used items at the top level
pub use aggregator::aggregate;
pub use baseline::{diff_against_baseline, Baseline, DiffResult};
pub use error::ParseError;
pub use parser::{parse_logs, parse_logs_one};
pub use types::{CuNode, CuReport, CuStats, LogEntry};
