//! External tool bridges for Mol Engine.
//!
//! This module contains bridges to external AI coding agents (OpenCode, etc.)
//! that can be invoked for complex code generation tasks ("beast mode").

pub mod opencode;

// Re-export the most commonly used types
pub use opencode::{
    ComplexityScore, OpenCodeBridge, OpenCodeResult, count_historical_failures, score_complexity,
};
