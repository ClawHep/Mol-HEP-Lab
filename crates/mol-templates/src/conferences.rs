//! Conference template definitions for Mol-HEP-Lab.
//!
//! Covers NeurIPS 2024/2025, ICML 2025/2026, and ICLR 2025/2026.
//! Style files (`.sty`) are **not** bundled; download URLs are embedded in
//! the generated preamble as comments.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Conference enum
// ---------------------------------------------------------------------------

/// Supported ML conference / year combinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conference {
    NeurIPS2024,
    NeurIPS2025,
    ICML2025,
    ICML2026,
    ICLR2025,
    ICLR2026,
}

impl Conference {
    /// All variants in a stable order.
    pub fn all() -> &'static [Conference] {
        &[
            Conference::NeurIPS2024,
            Conference::NeurIPS2025,
            Conference::ICML2025,
            Conference::ICML2026,
            Conference::ICLR2025,
            Conference::ICLR2026,
        ]
    }

    /// Human-readable display string.
    pub fn display_name(self) -> &'static str {
        match self {
            Conference::NeurIPS2024 => "NeurIPS 2024",
            Conference::NeurIPS2025 => "NeurIPS 2025",
            Conference::ICML2025 => "ICML 2025",
            Conference::ICML2026 => "ICML 2026",
            Conference::ICLR2025 => "ICLR 2025",
            Conference::ICLR2026 => "ICLR 2026",
        }
    }

    /// Official style-file download URL.
    pub fn style_download_url(self) -> &'static str {
        match self {
            Conference::NeurIPS2024 => {
                "https://media.neurips.cc/Conferences/NeurIPS2024/Styles.zip"
            }
            Conference::NeurIPS2025 => {
                "https://media.neurips.cc/Conferences/NeurIPS2025/Styles.zip"
            }
            Conference::ICML2025 => {
                "https://icml.cc/Conferences/2025/StyleAuthorInstructions"
            }
            Conference::ICML2026 => {
                "https://icml.cc/Conferences/2026/AuthorInstructions"
            }
            Conference::ICLR2025 => {
                "https://github.com/ICLR/Master-Template/raw/master/iclr2025.zip"
            }
            Conference::ICLR2026 => "https://github.com/ICLR/Master-Template",
        }
    }
}

impl std::fmt::Display for Conference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ---------------------------------------------------------------------------
// Section
// ---------------------------------------------------------------------------

/// A section expected (or required) in a conference paper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    /// Section heading, e.g. `"Introduction"`.
    pub title: String,
    /// Whether this section is mandatory for the submission.
    pub required: bool,
    /// Suggested maximum pages (informational; `None` means unconstrained).
    pub max_pages: Option<u32>,
    /// Brief description of what this section should contain.
    pub description: String,
}

impl Section {
    fn new(
        title: impl Into<String>,
        required: bool,
        max_pages: Option<u32>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            required,
            max_pages,
            description: description.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// ConferenceTemplate
// ---------------------------------------------------------------------------

/// Complete LaTeX template specification for a single conference / year.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceTemplate {
    /// Short machine-readable name, e.g. `"neurips_2025"`.
    pub name: String,
    /// The [`Conference`] this template targets.
    pub conference: Conference,
    /// Full LaTeX preamble (everything up to and including `\begin{document}`).
    pub preamble: String,
    /// Ordered list of paper sections.
    pub sections: Vec<Section>,
    /// Recommended page limit (main body, excluding references and appendix).
    pub page_limit: u32,
    /// Whether this is an anonymous (double-blind) submission template.
    pub anonymous: bool,
}

impl ConferenceTemplate {
    /// Render the footer block (bibliography + `\end{document}`).
    pub fn render_footer(&self, bib_file: &str) -> String {
        let bib_style = bib_style_for(self.conference);
        format!(
            "\n\\bibliographystyle{{{bib_style}}}\n\\bibliography{{{bib_file}}}\n\n\\end{{document}}\n"
        )
    }

    /// Render a minimal but complete `.tex` document skeleton.
    ///
    /// Sections appear as commented-out `\section{}` placeholders that the
    /// writing agent fills in.
    pub fn render_skeleton(&self, title: &str, authors: &str, abstract_text: &str) -> String {
        let body: String = self
            .sections
            .iter()
            .map(|s| {
                let req = if s.required { "required" } else { "optional" };
                format!(
                    "\n% --- {} ({}) ---\n% {}\n\\section{{{}}}\n\n",
                    s.title, req, s.description, s.title
                )
            })
            .collect();

        // Inject title / authors / abstract into the preamble
        let filled = self
            .preamble
            .replace("__TITLE__", title)
            .replace("__AUTHORS__", authors)
            .replace("__ABSTRACT__", abstract_text);

        format!("{filled}{body}{}", self.render_footer("references"))
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Return the [`ConferenceTemplate`] for the given [`Conference`].
pub fn get_template(conference: Conference) -> ConferenceTemplate {
    ConferenceTemplate {
        name: canonical_name(conference),
        conference,
        preamble: get_preamble(conference).to_owned(),
        sections: default_sections(conference),
        page_limit: page_limit(conference),
        anonymous: is_anonymous(conference),
    }
}

/// Return the LaTeX preamble string (static) for `conference`.
///
/// The preamble contains `__TITLE__`, `__AUTHORS__`, and `__ABSTRACT__`
/// placeholders that callers should substitute before use.
pub fn get_preamble(conference: Conference) -> &'static str {
    match conference {
        Conference::NeurIPS2024 => NEURIPS_2024_PREAMBLE,
        Conference::NeurIPS2025 => NEURIPS_2025_PREAMBLE,
        Conference::ICML2025 => ICML_2025_PREAMBLE,
        Conference::ICML2026 => ICML_2026_PREAMBLE,
        Conference::ICLR2025 => ICLR_2025_PREAMBLE,
        Conference::ICLR2026 => ICLR_2026_PREAMBLE,
    }
}

/// Return the default ordered sections for `conference`.
pub fn default_sections(conference: Conference) -> Vec<Section> {
    let mut sections = vec![
        Section::new(
            "Introduction",
            true,
            Some(1),
            "Motivate the problem, state contributions, and outline the paper.",
        ),
        Section::new(
            "Related Work",
            true,
            Some(1),
            "Survey relevant prior art and position this work relative to it.",
        ),
        Section::new(
            "Method",
            true,
            None,
            "Describe the proposed approach with sufficient technical detail.",
        ),
        Section::new(
            "Experiments",
            true,
            None,
            "Empirical evaluation: datasets, baselines, metrics, and ablations.",
        ),
        Section::new(
            "Results",
            true,
            None,
            "Present quantitative and qualitative results; compare with baselines.",
        ),
        Section::new(
            "Discussion",
            false,
            Some(1),
            "Interpret results, note limitations, and discuss broader impact.",
        ),
        Section::new(
            "Conclusion",
            true,
            Some(1),
            "Summarise contributions and suggest future directions.",
        ),
    ];

    // NeurIPS / ICLR: add a Broader Impact statement
    if matches!(
        conference,
        Conference::NeurIPS2024
            | Conference::NeurIPS2025
            | Conference::ICLR2025
            | Conference::ICLR2026
    ) {
        sections.push(Section::new(
            "Broader Impact",
            true,
            Some(1),
            "Discuss societal consequences, ethical considerations, and limitations.",
        ));
    }

    sections
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn canonical_name(c: Conference) -> String {
    match c {
        Conference::NeurIPS2024 => "neurips_2024",
        Conference::NeurIPS2025 => "neurips_2025",
        Conference::ICML2025 => "icml_2025",
        Conference::ICML2026 => "icml_2026",
        Conference::ICLR2025 => "iclr_2025",
        Conference::ICLR2026 => "iclr_2026",
    }
    .to_owned()
}

fn bib_style_for(c: Conference) -> &'static str {
    match c {
        Conference::NeurIPS2024 | Conference::NeurIPS2025 => "plainnat",
        Conference::ICML2025 => "icml2025",
        Conference::ICML2026 => "icml2026",
        Conference::ICLR2025 => "iclr2025_conference",
        Conference::ICLR2026 => "iclr2026_conference",
    }
}

fn page_limit(c: Conference) -> u32 {
    match c {
        Conference::NeurIPS2024 | Conference::NeurIPS2025 => 9,
        Conference::ICML2025 | Conference::ICML2026 => 8,
        Conference::ICLR2025 | Conference::ICLR2026 => 10,
    }
}

fn is_anonymous(c: Conference) -> bool {
    matches!(
        c,
        Conference::NeurIPS2024
            | Conference::NeurIPS2025
            | Conference::ICML2025
            | Conference::ICML2026
            | Conference::ICLR2025
            | Conference::ICLR2026
    )
}

// ---------------------------------------------------------------------------
// Preamble constants
// ---------------------------------------------------------------------------

static NEURIPS_2024_PREAMBLE: &str = r#"% Style file: https://media.neurips.cc/Conferences/NeurIPS2024/Styles.zip
\documentclass{article}
\usepackage[preprint]{neurips_2024}
\usepackage[utf8]{inputenc}
\usepackage[T1]{fontenc}
\usepackage{lmodern}
\usepackage{hyperref}
\usepackage{url}
\usepackage{booktabs}
\usepackage{amsfonts}
\usepackage{amsmath}
\usepackage{nicefrac}
\usepackage{microtype}
\usepackage{graphicx}
\usepackage{natbib}
\usepackage{algorithm}
\usepackage{algorithmic}
\usepackage{adjustbox}

\title{__TITLE__}
\author{__AUTHORS__}

\begin{document}
\maketitle

\begin{abstract}
__ABSTRACT__
\end{abstract}
"#;

static NEURIPS_2025_PREAMBLE: &str = r#"% Style file: https://media.neurips.cc/Conferences/NeurIPS2025/Styles.zip
\documentclass{article}
\usepackage[preprint]{neurips_2025}
\usepackage[utf8]{inputenc}
\usepackage[T1]{fontenc}
\usepackage{lmodern}
\usepackage{hyperref}
\usepackage{url}
\usepackage{booktabs}
\usepackage{amsfonts}
\usepackage{amsmath}
\usepackage{nicefrac}
\usepackage{microtype}
\usepackage{graphicx}
\usepackage{natbib}
\usepackage{algorithm}
\usepackage{algorithmic}
\usepackage{adjustbox}

\title{__TITLE__}
\author{__AUTHORS__}

\begin{document}
\maketitle

\begin{abstract}
__ABSTRACT__
\end{abstract}
"#;

static ICML_2025_PREAMBLE: &str = r#"% Style file: https://icml.cc/Conferences/2025/StyleAuthorInstructions
\documentclass{article}
\usepackage{icml2025}
\usepackage{hyperref}
\usepackage{url}
\usepackage{booktabs}
\usepackage{amsfonts}
\usepackage{amsmath}
\usepackage{nicefrac}
\usepackage{microtype}
\usepackage{graphicx}
\usepackage{natbib}
\usepackage{algorithm}
\usepackage{algorithmic}
\usepackage{adjustbox}

\icmltitlerunning{__TITLE__}

\begin{document}

\twocolumn[
\icmltitle{__TITLE__}

\begin{icmlauthorlist}
\icmlauthor{__AUTHORS__}{aff1}
\end{icmlauthorlist}
\icmlaffiliation{aff1}{Affiliation}

\icmlkeywords{machine learning, ICML}

\vskip 0.3in
]

\begin{abstract}
__ABSTRACT__
\end{abstract}
"#;

static ICML_2026_PREAMBLE: &str = r#"% Style file: https://icml.cc/Conferences/2026/AuthorInstructions
\documentclass{article}
\usepackage{icml2026}
\usepackage{hyperref}
\usepackage{url}
\usepackage{booktabs}
\usepackage{amsfonts}
\usepackage{amsmath}
\usepackage{nicefrac}
\usepackage{microtype}
\usepackage{graphicx}
\usepackage{natbib}
\usepackage{algorithm}
\usepackage{algorithmic}
\usepackage{adjustbox}

\icmltitlerunning{__TITLE__}

\begin{document}

\twocolumn[
\icmltitle{__TITLE__}

\begin{icmlauthorlist}
\icmlauthor{__AUTHORS__}{aff1}
\end{icmlauthorlist}
\icmlaffiliation{aff1}{Affiliation}

\icmlkeywords{machine learning, ICML}

\vskip 0.3in
]

\begin{abstract}
__ABSTRACT__
\end{abstract}
"#;

static ICLR_2025_PREAMBLE: &str = r#"% Style file: https://github.com/ICLR/Master-Template/raw/master/iclr2025.zip
\documentclass{article}
\usepackage{iclr2025_conference}
\usepackage{hyperref}
\usepackage{url}
\usepackage{booktabs}
\usepackage{amsfonts}
\usepackage{amsmath}
\usepackage{nicefrac}
\usepackage{microtype}
\usepackage{graphicx}
\usepackage{natbib}
\usepackage{algorithm}
\usepackage{algorithmic}
\usepackage{adjustbox}

\title{__TITLE__}
\author{__AUTHORS__}

\begin{document}
\maketitle

\begin{abstract}
__ABSTRACT__
\end{abstract}
"#;

static ICLR_2026_PREAMBLE: &str = r#"% Style file: https://github.com/ICLR/Master-Template
\documentclass{article}
\usepackage{iclr2026_conference}
\usepackage{hyperref}
\usepackage{url}
\usepackage{booktabs}
\usepackage{amsfonts}
\usepackage{amsmath}
\usepackage{nicefrac}
\usepackage{microtype}
\usepackage{graphicx}
\usepackage{natbib}
\usepackage{algorithm}
\usepackage{algorithmic}
\usepackage{adjustbox}

\title{__TITLE__}
\author{__AUTHORS__}

\begin{document}
\maketitle

\begin{abstract}
__ABSTRACT__
\end{abstract}
"#;
