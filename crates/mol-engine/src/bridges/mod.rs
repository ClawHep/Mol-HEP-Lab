//! External tool bridges for Mol Engine.
//!
//! This module contains bridges to external AI coding agents (OpenCode, Aider, etc.)
//! that can be invoked for complex code generation tasks ("beast mode").

pub mod common;
pub mod opencode;
pub mod openhands;

pub use common::{copy_dir_filtered, md5_hex};

// Re-export the most commonly used types
pub use opencode::{
    ComplexityScore, OpenCodeBridge, OpenCodeResult, count_historical_failures, score_complexity,
};
pub use openhands::OpenHandsBridge;
