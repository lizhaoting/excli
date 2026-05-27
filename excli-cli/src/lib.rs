//! excli-cli: CLI entry point, argument parsing, and output formatting.

pub mod cli;
pub mod commands;
pub mod output;
pub mod progress;

pub use cli::build_cli;
pub use commands::dispatch;
pub use output::{format_output, OutputFormat};
pub use progress::ProgressTracker;
