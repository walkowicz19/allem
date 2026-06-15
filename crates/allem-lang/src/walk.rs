//! Minimal recursive source-file walker. No extra dependencies (std only). Skips common
//! vendor/build/VCS directories plus any user-configured `exclude` fragments (KISS).

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

/// Collect all files under `root` (recursively), skipping ignored/excluded directories.
pub fn source_files(root: &Path, exclude: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(root, exclude, &mut out);
    out
}

/// Read every recognized source file under `root` as `(path, contents)`, honoring excludes.
/// Shared by the cross-file passes so they don't each re-implement the walk+read (DRY).
pub fn read_sources(root: &Path, exclude: &[String]) -> Vec<(PathBuf, String)> {
    source_files(root, exclude)
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|s| (p, s)))
        .collect()
}

/// A path is excluded if any configured fragment appears in its slash-normalized form.
pub fn is_excluded(path: &Path, exclude: &[String]) -> bool {
    if exclude.is_empty() {
        return false;
    }
    let normalized = path.to_string_lossy().replace('\\', "/");
    exclude.iter().any(|frag| {
        let frag = frag.replace('\\', "/");
        !frag.is_empty() && normalized.contains(&frag)
    })
}

fn collect(dir: &Path, exclude: &[String], out: &mut Vec<PathBuf>) {
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
            if !skip && !is_excluded(&path, exclude) {
                collect(&path, exclude, out);
            }
        } else if file_type.is_file() && !is_excluded(&path, exclude) {
            out.push(path);
        }
    }
}
