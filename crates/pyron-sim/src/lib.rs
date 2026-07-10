pub mod detector;
pub mod loader;
pub mod runner;

// Re-export the most used types
pub use detector::{detect, Framework, ProjectInfo};
pub use loader::{anchor_discriminator, load_idl, IdlInstruction};
pub use runner::{simulate_all, simulate_instruction, RunConfig};
