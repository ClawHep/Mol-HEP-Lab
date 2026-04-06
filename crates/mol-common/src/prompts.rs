//! Prompt template engine for MolAgent.
//!
//! Wraps the [`tera`] template engine and provides a thin, ergonomic API for
//! loading prompt templates from disk and rendering them with variable
//! substitution.
//!
//! # Example
//!
//! ```no_run
//! use mol_common::prompts::PromptEngine;
//!
//! let engine = PromptEngine::from_directory("prompts/").unwrap();
//! let vars = [("topic".to_string(), tera::Value::String("HEP jets".to_string()))];
//! let rendered = engine.render("synthesis.md", vars).unwrap();
//! ```

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use tera::{Context as TeraContext, Tera, Value};

/// Template rendering engine backed by [`tera`].
///
/// Templates use the Jinja2-compatible `{{ variable }}` syntax.
pub struct PromptEngine {
    tera: Tera,
}

impl PromptEngine {
    /// Load all `*.md`, `*.txt`, and `*.yaml` templates from `dir`.
    ///
    /// The template names are the file names relative to `dir` with forward
    /// slashes as separators (e.g. `"synthesis.md"`).
    pub fn from_directory(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let glob = format!("{}/**/*.{{md,txt,yaml,yml,j2}}", dir.display());
        let tera = Tera::new(&glob)
            .with_context(|| format!("failed to load prompt templates from {}", dir.display()))?;
        Ok(Self { tera })
    }

    /// Build an engine from a collection of `(name, template_source)` pairs.
    ///
    /// Useful for embedding templates directly in the binary or in tests.
    pub fn from_strings<I, N, S>(templates: I) -> Result<Self>
    where
        I: IntoIterator<Item = (N, S)>,
        N: AsRef<str>,
        S: AsRef<str>,
    {
        let mut tera = Tera::default();
        let pairs: Vec<(&str, &str)> = templates
            .into_iter()
            .map(|(n, s)| {
                // Leak the strings for the lifetime of the call — we collect below.
                let name: &'static str = Box::leak(n.as_ref().to_owned().into_boxed_str());
                let src: &'static str = Box::leak(s.as_ref().to_owned().into_boxed_str());
                (name, src)
            })
            .collect();
        tera.add_raw_templates(pairs)
            .context("failed to add prompt templates")?;
        Ok(Self { tera })
    }

    /// Render a template by name with the given variables.
    ///
    /// `vars` is any iterable of `(key, value)` pairs where the value is a
    /// [`tera::Value`].
    pub fn render<I, K>(&self, template_name: &str, vars: I) -> Result<String>
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        let mut ctx = TeraContext::new();
        for (k, v) in vars {
            ctx.insert(k.into(), &v);
        }
        self.tera
            .render(template_name, &ctx)
            .with_context(|| format!("failed to render template '{template_name}'"))
    }

    /// Render a template with simple string-to-string substitution.
    ///
    /// All values are inserted as JSON strings.
    pub fn render_str_vars<I, K, V>(
        &self,
        template_name: &str,
        vars: I,
    ) -> Result<String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let typed: Vec<(String, Value)> = vars
            .into_iter()
            .map(|(k, v)| (k.into(), Value::String(v.into())))
            .collect();
        self.render(template_name, typed)
    }

    /// Render an ad-hoc template string (not loaded from disk) with the given
    /// variables.  Useful for one-off dynamic templates.
    pub fn render_one_shot<I, K>(template_source: &str, vars: I) -> Result<String>
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        let mut tera = Tera::default();
        tera.add_raw_template("__one_shot__", template_source)
            .context("failed to parse one-shot template")?;
        let mut ctx = TeraContext::new();
        for (k, v) in vars {
            ctx.insert(k.into(), &v);
        }
        tera.render("__one_shot__", &ctx)
            .context("failed to render one-shot template")
    }

    /// Render an ad-hoc template with simple string substitution.
    pub fn render_one_shot_str<I, K, V>(template_source: &str, vars: I) -> Result<String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let typed: Vec<(String, Value)> = vars
            .into_iter()
            .map(|(k, v)| (k.into(), Value::String(v.into())))
            .collect();
        Self::render_one_shot(template_source, typed)
    }

    /// List all template names registered in this engine.
    pub fn template_names(&self) -> Vec<&str> {
        self.tera.get_template_names().collect()
    }

    /// Check whether a template with the given name is registered.
    pub fn has_template(&self, name: &str) -> bool {
        self.tera.get_template_names().any(|n| n == name)
    }

    /// Add or replace a template at runtime.
    pub fn add_template(&mut self, name: &str, source: &str) -> Result<()> {
        self.tera
            .add_raw_template(name, source)
            .with_context(|| format!("failed to add template '{name}'"))
    }

    /// Build a [`tera::Context`] from a [`HashMap<String, String>`].
    pub fn context_from_map(map: &HashMap<String, String>) -> TeraContext {
        let mut ctx = TeraContext::new();
        for (k, v) in map {
            ctx.insert(k.as_str(), v);
        }
        ctx
    }
}

impl std::fmt::Debug for PromptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.tera.get_template_names().collect();
        f.debug_struct("PromptEngine")
            .field("templates", &names)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_string_substitution() {
        let result = PromptEngine::render_one_shot_str(
            "Hello, {{ name }}! Research topic: {{ topic }}.",
            [("name", "MolAgent"), ("topic", "HEP jet tagging")],
        )
        .unwrap();
        assert_eq!(result, "Hello, MolAgent! Research topic: HEP jet tagging.");
    }

    #[test]
    fn from_strings_round_trip() {
        let engine = PromptEngine::from_strings([("hello.md", "Hi {{ name }}!")]).unwrap();
        assert!(engine.has_template("hello.md"));
        let out = engine
            .render_str_vars("hello.md", [("name", "World")])
            .unwrap();
        assert_eq!(out, "Hi World!");
    }
}
