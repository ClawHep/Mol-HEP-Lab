//! Publication-quality style presets for figure generation.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// StyleConfig
// ---------------------------------------------------------------------------

/// Publication style preset controlling figure aesthetics.
///
/// Matches the matplotlib rcParams and TikZ style options used by the
/// Python figure agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleConfig {
    /// Output DPI (e.g. 300 for print quality).
    pub dpi: u32,

    /// Figure width in inches.
    pub figure_width_in: f32,

    /// Figure height in inches.
    pub figure_height_in: f32,

    /// Font family (e.g. `"serif"`, `"sans-serif"`, `"monospace"`).
    pub font_family: String,

    /// Font size in points.
    pub font_size_pt: u32,

    /// Axes label font size.
    pub label_size_pt: u32,

    /// Tick label font size.
    pub tick_size_pt: u32,

    /// Legend font size.
    pub legend_size_pt: u32,

    /// Line width in points.
    pub line_width_pt: f32,

    /// Marker size.
    pub marker_size: f32,

    /// Colour cycle to use (e.g. `"tab10"`, `"Set2"`, `"viridis"`).
    pub color_cycle: String,

    /// Background colour (hex or CSS name).
    pub background_color: String,

    /// Whether to use LaTeX rendering for text.
    pub use_latex: bool,

    /// Grid style: `"none"`, `"major"`, `"both"`.
    pub grid: String,

    /// Whether to apply a tight layout.
    pub tight_layout: bool,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self::arxiv()
    }
}

impl StyleConfig {
    // -- Presets ------------------------------------------------------------

    /// ArXiv / IEEE preprint style (double-column, 300 DPI).
    pub fn arxiv() -> Self {
        Self {
            dpi: 300,
            figure_width_in: 3.5,
            figure_height_in: 2.6,
            font_family: "serif".to_owned(),
            font_size_pt: 9,
            label_size_pt: 9,
            tick_size_pt: 8,
            legend_size_pt: 8,
            line_width_pt: 1.0,
            marker_size: 4.0,
            color_cycle: "tab10".to_owned(),
            background_color: "white".to_owned(),
            use_latex: true,
            grid: "major".to_owned(),
            tight_layout: true,
        }
    }

    /// Nature / Science style (single-column, 600 DPI).
    pub fn nature() -> Self {
        Self {
            dpi: 600,
            figure_width_in: 3.46,
            figure_height_in: 2.6,
            font_family: "sans-serif".to_owned(),
            font_size_pt: 7,
            label_size_pt: 7,
            tick_size_pt: 6,
            legend_size_pt: 6,
            line_width_pt: 0.8,
            marker_size: 3.0,
            color_cycle: "Set2".to_owned(),
            background_color: "white".to_owned(),
            use_latex: false,
            grid: "none".to_owned(),
            tight_layout: true,
        }
    }

    /// Presentation / slide style (large, dark background).
    pub fn presentation() -> Self {
        Self {
            dpi: 150,
            figure_width_in: 10.0,
            figure_height_in: 6.0,
            font_family: "sans-serif".to_owned(),
            font_size_pt: 16,
            label_size_pt: 14,
            tick_size_pt: 12,
            legend_size_pt: 12,
            line_width_pt: 2.5,
            marker_size: 8.0,
            color_cycle: "tab10".to_owned(),
            background_color: "#1a1a2e".to_owned(),
            use_latex: false,
            grid: "both".to_owned(),
            tight_layout: false,
        }
    }

    // -- Codegen helper -----------------------------------------------------

    /// Emit matplotlib rcParams setup as a Python snippet.
    pub fn to_matplotlib_rc(&self) -> String {
        let latex_str = if self.use_latex { "True" } else { "False" };
        format!(
            r#"import matplotlib.pyplot as plt
import matplotlib as mpl

mpl.rcParams.update({{
    'figure.dpi': {dpi},
    'figure.figsize': ({width}, {height}),
    'font.family': '{font_family}',
    'font.size': {font_size},
    'axes.labelsize': {label_size},
    'xtick.labelsize': {tick_size},
    'ytick.labelsize': {tick_size},
    'legend.fontsize': {legend_size},
    'lines.linewidth': {line_width},
    'lines.markersize': {marker_size},
    'axes.prop_cycle': mpl.cycler('color', plt.cm.{color_cycle}.colors if hasattr(plt.cm.{color_cycle}, 'colors') else plt.get_cmap('{color_cycle}').colors),
    'text.usetex': {use_latex},
    'axes.grid': {grid},
    'figure.facecolor': '{background_color}',
}})
"#,
            dpi = self.dpi,
            width = self.figure_width_in,
            height = self.figure_height_in,
            font_family = self.font_family,
            font_size = self.font_size_pt,
            label_size = self.label_size_pt,
            tick_size = self.tick_size_pt,
            legend_size = self.legend_size_pt,
            line_width = self.line_width_pt,
            marker_size = self.marker_size,
            color_cycle = self.color_cycle,
            use_latex = latex_str,
            grid = if self.grid == "none" { "False" } else { "True" },
            background_color = self.background_color,
        )
    }
}
