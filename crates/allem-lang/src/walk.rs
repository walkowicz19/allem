//! Minimal recursive source-file walker. No extra dependencies (std only). Skips common
//! vendor/build/VCS directories so analysis stays fast and relevant (KISS).

use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "dist",
    "build",
    "vendor",
];

/// Collect all files under `root` (recursively), skipping ignored directories.
pub fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(root, &mut out);
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let skip = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| SKIP_DIRS.contains(&n))
                .unwrap_or(false);
            if !skip {
                collect(&path, out);
            }
        } else if file_type.is_file() {
            out.push(path);
        }
    }
}
