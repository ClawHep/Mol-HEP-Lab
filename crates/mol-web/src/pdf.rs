//! PDF text and metadata extraction via lopdf.
//!
//! Extracts plain text, page count, and available metadata (Title, Author,
//! Subject) from PDF files.
//!
//! # Usage
//!
//! ```no_run
//! use std::path::Path;
//! use mol_web::pdf::extract_text;
//!
//! let content = extract_text(Path::new("/tmp/paper.pdf")).unwrap();
//! println!("Pages: {}", content.pages);
//! println!("Text preview: {}", &content.text[..200.min(content.text.len())]);
//! ```

use anyhow::Result;
use lopdf::Document;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Metadata fields extracted from a PDF document catalogue.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PdfMetadata {
    /// Document title (from PDF metadata).
    pub title: Option<String>,
    /// Author string as it appears in the PDF metadata.
    pub author: Option<String>,
    /// Subject / keywords field.
    pub subject: Option<String>,
    /// PDF producer application.
    pub producer: Option<String>,
    /// PDF creator application.
    pub creator: Option<String>,
    /// Additional raw metadata key-value pairs.
    pub extra: HashMap<String, String>,
}

/// Content extracted from a PDF file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfContent {
    /// Full concatenated text of all extracted pages.
    pub text: String,
    /// Total number of pages in the document.
    pub pages: u32,
    /// Structured metadata from the PDF catalogue.
    pub metadata: PdfMetadata,
    /// Per-page text, indexed 0-based.
    pub page_texts: Vec<String>,
    /// Whether extraction succeeded.
    pub success: bool,
    /// Error description if `success` is false.
    pub error: String,
}

impl PdfContent {
    /// Returns `true` if the document contains non-trivial text.
    pub fn has_content(&self) -> bool {
        self.success && self.text.trim().len() > 100
    }

    /// Convenience accessor: authors parsed from the `author` metadata field
    /// (comma-separated).
    pub fn authors(&self) -> Vec<String> {
        self.metadata
            .author
            .as_deref()
            .map(|a| a.split(',').map(|s| s.trim().to_owned()).collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/// Extract text and metadata from a PDF file at `path`.
///
/// Returns a populated [`PdfContent`] on success. On failure the returned
/// struct has `success = false` and an error message; the function itself only
/// returns `Err` for truly unrecoverable I/O problems (e.g., file not found).
pub fn extract_text(path: &Path) -> Result<PdfContent> {
    if !path.exists() {
        return Ok(PdfContent {
            text: String::new(),
            pages: 0,
            metadata: PdfMetadata::default(),
            page_texts: Vec::new(),
            success: false,
            error: format!("file not found: {}", path.display()),
        });
    }

    let doc = match Document::load(path) {
        Ok(d) => d,
        Err(e) => {
            warn!("lopdf failed to load {}: {e}", path.display());
            return Ok(PdfContent {
                text: String::new(),
                pages: 0,
                metadata: PdfMetadata::default(),
                page_texts: Vec::new(),
                success: false,
                error: e.to_string(),
            });
        }
    };

    let pages = doc.get_pages();
    let page_count = pages.len() as u32;
    debug!(
        "PDF {}: {} pages",
        path.file_name().unwrap_or_default().to_string_lossy(),
        page_count
    );

    // Extract per-page text
    let mut page_texts: Vec<String> = Vec::with_capacity(page_count as usize);
    for (page_num, _page_id) in &pages {
        match doc.extract_text(&[*page_num]) {
            Ok(text) => page_texts.push(text),
            Err(e) => {
                debug!("Page {page_num} text extraction failed: {e}");
                page_texts.push(String::new());
            }
        }
    }

    let text = page_texts.join("\n");

    // Extract metadata from Info dictionary
    let metadata = extract_metadata(&doc);

    Ok(PdfContent {
        text,
        pages: page_count,
        metadata,
        page_texts,
        success: true,
        error: String::new(),
    })
}

/// Extract a [`PdfContent`] from raw PDF bytes (e.g., downloaded from a URL).
pub fn extract_text_from_bytes(bytes: &[u8]) -> Result<PdfContent> {
    let doc = match Document::load_mem(bytes) {
        Ok(d) => d,
        Err(e) => {
            warn!("lopdf failed to load PDF from bytes: {e}");
            return Ok(PdfContent {
                text: String::new(),
                pages: 0,
                metadata: PdfMetadata::default(),
                page_texts: Vec::new(),
                success: false,
                error: e.to_string(),
            });
        }
    };

    let pages = doc.get_pages();
    let page_count = pages.len() as u32;

    let mut page_texts: Vec<String> = Vec::with_capacity(page_count as usize);
    for (page_num, _page_id) in &pages {
        match doc.extract_text(&[*page_num]) {
            Ok(text) => page_texts.push(text),
            Err(e) => {
                debug!("Page {page_num} text extraction failed: {e}");
                page_texts.push(String::new());
            }
        }
    }

    let text = page_texts.join("\n");
    let metadata = extract_metadata(&doc);

    Ok(PdfContent {
        text,
        pages: page_count,
        metadata,
        page_texts,
        success: true,
        error: String::new(),
    })
}

// ---------------------------------------------------------------------------
// Metadata extraction helper
// ---------------------------------------------------------------------------

fn extract_metadata(doc: &Document) -> PdfMetadata {
    let mut meta = PdfMetadata::default();

    // lopdf stores PDF Info dictionary at the trailer's /Info reference.
    let info_id = match doc.trailer.get(b"Info") {
        Ok(obj) => obj.as_reference().ok(),
        Err(_) => None,
    };

    let info_dict = info_id
        .and_then(|id| doc.get_object(id).ok())
        .and_then(|obj| obj.as_dict().ok().cloned());

    if let Some(dict) = info_dict {
        for (key, value) in dict.iter() {
            let k = String::from_utf8_lossy(key).to_string();
            let v = pdf_string_to_string(value);

            match k.as_str() {
                "Title" => meta.title = Some(v),
                "Author" => meta.author = Some(v),
                "Subject" => meta.subject = Some(v),
                "Producer" => meta.producer = Some(v),
                "Creator" => meta.creator = Some(v),
                _ => {
                    meta.extra.insert(k, v);
                }
            }
        }
    }

    meta
}

/// Convert a lopdf Object to a String best-effort.
fn pdf_string_to_string(obj: &lopdf::Object) -> String {
    match obj {
        lopdf::Object::String(bytes, _) => {
            // PDF strings can be PDFDocEncoding or UTF-16BE
            if bytes.starts_with(&[0xFE, 0xFF]) {
                // UTF-16BE BOM
                let chars: Vec<u16> = bytes[2..]
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16_lossy(&chars)
            } else {
                String::from_utf8_lossy(bytes).to_string()
            }
        }
        lopdf::Object::Name(bytes) => String::from_utf8_lossy(bytes).to_string(),
        other => format!("{other:?}"),
    }
}
