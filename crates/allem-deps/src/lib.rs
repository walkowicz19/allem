//! `allem-deps` — dependency intelligence: manifest parsing plus the
//! outdated / vulnerable / dangerous / injection checks, per ecosystem.
//!
//! The engine stays ecosystem-agnostic: it asks each adapter to detect, parse, and check.
//! Every issue is emitted as a reviewable candidate `Finding` (triage-first, never bulk-fixed).

pub mod cargo;
pub mod danger;
pub mod fix;
pub mod model;
pub mod osv;
pub mod outdated;
pub mod python;

use allem_core::{
    Category, Confidence, Config, Finding, PackageRef, Result, Severity, SuggestedAction,
};
use model::{Advisory, Package};
use osv::{OfflineSource, OsvClient, VulnSource};
use outdated::{OfflineRegistry, OnlineRegistry, RegistrySource};
use std::path::Path;

/// Detect dependency ecosystems present under `root` (by OSV ecosystem id).
pub fn detect_ecosystems(root: &Path) -> Vec<&'static str> {
    let mut found = Vec::new();
    if python::detect(root) {
        found.push(python::ECOSYSTEM);
    }
    if cargo::detect(root) {
        found.push(cargo::ECOSYSTEM);
    }
    found
}

/// Run dependency analysis for every detected ecosystem under `root`.
pub fn analyze(root: &Path, config: &Config) -> Result<Vec<Finding>> {
    let (vuln, registry): (Box<dyn VulnSource>, Box<dyn RegistrySource>) = if config.offline {
        (Box::new(OfflineSource), Box::new(OfflineRegistry))
    } else {
        (Box::new(OsvClient::default()), Box::new(OnlineRegistry))
    };
    analyze_with(root, vuln.as_ref(), registry.as_ref())
}

/// Same as [`analyze`], but with injected sources (so tests stay offline & hermetic).
pub fn analyze_with(
    root: &Path,
    vuln: &dyn VulnSource,
    registry: &dyn RegistrySource,
) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();

    // --- Python ---
    if python::detect(root) {
        let manifest = "requirements.txt";
        let path = root.join(manifest);
        let text = read(&path)?;
        let packages = python::parse_requirements(manifest, &text);
        findings.extend(run_checks(&packages, vuln, registry)?);
        findings.extend(danger::scan_injection(manifest, &text));
        // setup.py is a common install-time execution vector.
        if let Ok(setup) = std::fs::read_to_string(root.join("setup.py")) {
            findings.extend(danger::scan_injection("setup.py", &setup));
        }
    }

    // --- Rust ---
    if cargo::detect(root) {
        let manifest = "Cargo.toml";
        let path = root.join(manifest);
        let text = read(&path)?;
        let packages = cargo::parse_cargo_toml(manifest, &text)?;
        findings.extend(run_checks(&packages, vuln, registry)?);
    }

    Ok(findings)
}

/// The shared per-ecosystem check pipeline: danger heuristics + outdated + OSV vulnerabilities.
fn run_checks(
    packages: &[Package],
    vuln: &dyn VulnSource,
    registry: &dyn RegistrySource,
) -> Result<Vec<Finding>> {
    let mut findings = danger::check_packages(packages);
    findings.extend(outdated::check_outdated(packages, registry)?);
    for pkg in packages {
        for adv in vuln.query(pkg)? {
            findings.push(vuln_finding(pkg, &adv));
        }
    }
    Ok(findings)
}

fn vuln_finding(pkg: &Package, adv: &Advisory) -> Finding {
    let severity = map_severity(adv.severity.as_deref());
    let mut f = Finding::new(
        format!("dep/{}/{}/{}", pkg.ecosystem, pkg.name, adv.id),
        Category::DependencyVulnerable,
        severity,
        if adv.summary.is_empty() {
            format!(
                "{} {} is affected by {}",
                pkg.name,
                pkg.version.raw(),
                adv.id
            )
        } else {
            format!("{}: {}", adv.id, adv.summary)
        },
    )
    .with_package(PackageRef {
        ecosystem: pkg.ecosystem.to_string(),
        name: pkg.name.clone(),
        version: pkg.version.raw().to_string(),
    })
    .with_evidence("advisory", adv.id.clone().into());

    if let Some(fixed) = &adv.fixed_in {
        f = f
            .with_evidence("fixed_in", fixed.clone().into())
            .with_action(SuggestedAction {
                action_type: "upgrade".into(),
                to: Some(fixed.clone()),
                confidence: Confidence::High,
            });
    } else {
        f = f.with_action(SuggestedAction {
            action_type: "review".into(),
            to: None,
            confidence: Confidence::Medium,
        });
    }
    f
}

fn map_severity(label: Option<&str>) -> Severity {
    match label.map(str::to_ascii_uppercase).as_deref() {
        Some("CRITICAL") => Severity::Critical,
        Some("HIGH") => Severity::High,
        Some("MODERATE" | "MEDIUM") => Severity::Medium,
        Some("LOW") => Severity::Low,
        // Unknown/absent: a known advisory is still High by default — fail-safe on severity.
        _ => Severity::High,
    }
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| allem_core::AllemError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use model::VersionSpec;

    struct FakeVuln;
    impl VulnSource for FakeVuln {
        fn query(&self, package: &Package) -> Result<Vec<Advisory>> {
            if package.name == "requests" {
                Ok(vec![Advisory {
                    id: "CVE-2024-0001".into(),
                    summary: "test advisory".into(),
                    fixed_in: Some("2.32.0".into()),
                    severity: Some("HIGH".into()),
                }])
            } else {
                Ok(Vec::new())
            }
        }
    }

    #[test]
    fn vuln_source_produces_upgrade_action() {
        let pkgs = vec![Package {
            ecosystem: "PyPI",
            name: "requests".into(),
            version: VersionSpec::Pinned("2.19.0".into()),
            manifest: "requirements.txt".into(),
            line: Some(1),
        }];
        let findings = run_checks(&pkgs, &FakeVuln, &OfflineRegistry).unwrap();
        let v = findings
            .iter()
            .find(|f| f.category == Category::DependencyVulnerable)
            .unwrap();
        assert_eq!(v.severity, Severity::High);
        let action = v.suggested_action.as_ref().unwrap();
        assert_eq!(action.action_type, "upgrade");
        assert_eq!(action.to.as_deref(), Some("2.32.0"));
        assert!(!v.auto_applied);
    }
}
