//! Mol-HEP-Lab: LaTeX template compilation and rendering for ML conference papers.
//!
//! This crate provides three main capabilities:
//!
//! - **[`compiler`]** — Async LaTeX compilation (pdflatex → bibtex → pdflatex ×2)
//!   and post-compilation quality checks.
//! - **[`conferences`]** — Structured templates for NeurIPS, ICML, and ICLR
//!   (2024–2026), including preambles, section outlines, and page limits.
//! - **[`converter`]** — Markdown-to-LaTeX conversion for agent-generated content.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use mol_templates::{
//!     compiler::compile_latex,
//!     conferences::{get_template, Conference},
//!     converter::markdown_to_latex,
//! };
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // 1. Pick a conference template
//! let tmpl = get_template(Conference::NeurIPS2025);
//!
//! // 2. Generate a .tex skeleton
//! let tex = tmpl.render_skeleton("My Paper", "Author One", "This paper shows…");
//!
//! // 3. Convert a Markdown section to LaTeX body text
//! let body = markdown_to_latex("## Introduction\n\nWe propose **MolNet** [@molnet2025].");
//!
//! // 4. Compile the document
//! let result = compile_latex(Path::new("/tmp/paper.tex"), Path::new("/tmp")).await?;
//! assert!(result.success);
//! # Ok(())
//! # }
//! ```

pub mod compiler;
pub mod conferences;
pub mod converter;

// Re-export the most commonly used items at crate root for convenience.
pub use compiler::{check_quality, compile_latex, CompileResult, QualityCheckResult};
pub use conferences::{
    default_sections, get_preamble, get_template, Conference, ConferenceTemplate, Section,
};
pub use converter::markdown_to_latex;
