use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("log line has unexpected format: {0}")]
    MalformedLog(String),

    #[error("consumed line appeared with no open frame")]
    UnexpectedConsumed,

    #[error("invoke opened but never closed for program: {0}")]
    UnclosedFrame(String),

    #[error("no frames found in logs — was the transaction simulated correctly?")]
    EmptyLogs,
}
