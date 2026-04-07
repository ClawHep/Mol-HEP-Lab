//! Shared utility helpers used by multiple bridge modules.

use std::path::Path;

use md5::{Digest, Md5};

/// Compute the MD5 hex digest of `bytes`.
pub fn md5_hex(bytes: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Copy all contents of `src` directory into `dst`, skipping common noise:
/// `.git`, `__pycache__`, `node_modules`, `.eggs`, `_manifest.json`, `*.pyc`.
pub fn copy_dir_filtered(src: &Path, dst: &Path) -> std::io::Result<()> {
    const SKIP: &[&str] = &[
        ".git",
        "__pycache__",
        "node_modules",
        ".eggs",
        "_manifest.json",
    ];

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip noise
        if SKIP.iter().any(|&s| name_str == s) {
            continue;
        }
        if name_str.ends_with(".pyc") {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(&name);

        if src_path.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_filtered(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
