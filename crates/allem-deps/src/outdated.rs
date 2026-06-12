//! Outdated-dependency detection. `RegistrySource` is a trait (like [`crate::osv::VulnSource`])
//! so the engine is testable offline and data sources are swappable. The live source queries
//! each ecosystem's registry for the latest stable version; `semver` decides how far behind a
//! pinned version is, which sets severity. Network failures fail-open (no finding, never a crash).

use crate::model::Package;
use allem_core::{Category, Confidence, Finding, PackageRef, Result, Severity, SuggestedAction};
use semver::Version;

/// A source of the latest stable version for a package.
pub trait RegistrySource: Send + Sync {
    fn latest(&self, package: &Package) -> Result<Option<String>>;
}

/// Offline / disabled source — reports nothing (keeps runs hermetic in tests/offline mode).
pub struct OfflineRegistry;

impl RegistrySource for OfflineRegistry {
    fn latest(&self, _package: &Package) -> Result<Option<String>> {
        Ok(None)
    }
}

/// Live source dispatching by ecosystem to the public registry APIs.
pub struct OnlineRegistry;

impl RegistrySource for OnlineRegistry {
    fn latest(&self, package: &Package) -> Result<Option<String>> {
        let version = match package.ecosystem {
            "PyPI" => pypi_latest(&package.name),
            "crates.io" => crates_latest(&package.name),
            _ => None,
        };
        Ok(version)
    }
}

fn pypi_latest(name: &str) -> Option<String> {
    let url = format!("https://pypi.org/pypi/{name}/json");
    let value: serde_json::Value = ureq::get(&url).call().ok()?.into_json().ok()?;
    value
        .get("info")?
        .get("version")?
        .as_str()
        .map(str::to_string)
}

fn crates_latest(name: &str) -> Option<String> {
    // crates.io requires a descriptive User-Agent.
    let url = format!("https://crates.io/api/v1/crates/{name}");
    let value: serde_json::Value = ureq::get(&url)
        .set(
            "User-Agent",
            "allem/0.1 (https://github.com/walkowicz19/allem)",
        )
        .call()
        .ok()?
        .into_json()
        .ok()?;
    let krate = value.get("crate")?;
    krate
        .get("max_stable_version")
        .or_else(|| krate.get("newest_version"))?
        .as_str()
        .map(str::to_string)
}

/// Emit `DependencyOutdated` findings for pinned packages behind the latest stable release.
pub fn check_outdated(packages: &[Package], registry: &dyn RegistrySource) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    for pkg in packages {
        let Some(current_raw) = pkg.version.concrete() else {
            continue; // only compare concrete pins; ranges/wildcards handled by danger checks
        };
        let Some(latest_raw) = registry.latest(pkg)? else {
            continue;
        };
        if let Some(finding) = compare(pkg, current_raw, &latest_raw) {
            findings.push(finding);
        }
    }
    Ok(findings)
}

/// Build a finding if `latest` is strictly newer than `current`; severity scales with the gap.
fn compare(pkg: &Package, current: &str, latest: &str) -> Option<Finding> {
    let cur = Version::parse(current).ok()?;
    let new = Version::parse(latest).ok()?;
    if new <= cur {
        return None;
    }
    let severity = if new.major > cur.major {
        Severity::Medium
    } else if new.minor > cur.minor {
        Severity::Low
    } else {
        Severity::Info
    };
    Some(
        Finding::new(
            format!("dep/outdated/{}/{}", pkg.ecosystem, pkg.name),
            Category::DependencyOutdated,
            severity,
            format!("`{}` is outdated: {current} → {latest} available", pkg.name),
        )
        .with_package(PackageRef {
            ecosystem: pkg.ecosystem.to_string(),
            name: pkg.name.clone(),
            version: current.to_string(),
        })
        .with_evidence("latest", latest.into())
        .with_action(SuggestedAction {
            action_type: "upgrade".into(),
            to: Some(latest.to_string()),
            confidence: Confidence::High,
        }),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::VersionSpec;

    struct FakeRegistry(&'static str);
    impl RegistrySource for FakeRegistry {
        fn latest(&self, _package: &Package) -> Result<Option<String>> {
            Ok(Some(self.0.to_string()))
        }
    }

    fn pkg(version: &str) -> Package {
        Package {
            ecosystem: "PyPI",
            name: "requests".into(),
            version: VersionSpec::Pinned(version.into()),
            manifest: "requirements.txt".into(),
            line: Some(1),
        }
    }

    #[test]
    fn major_gap_is_medium() {
        let f = check_outdated(&[pkg("1.0.0")], &FakeRegistry("3.1.0")).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Medium);
        assert_eq!(f[0].category, Category::DependencyOutdated);
    }

    #[test]
    fn patch_gap_is_info() {
        let f = check_outdated(&[pkg("2.0.0")], &FakeRegistry("2.0.3")).unwrap();
        assert_eq!(f[0].severity, Severity::Info);
    }

    #[test]
    fn up_to_date_is_clean() {
        let f = check_outdated(&[pkg("2.0.0")], &FakeRegistry("2.0.0")).unwrap();
        assert!(f.is_empty());
    }

    #[test]
    fn offline_registry_reports_nothing() {
        let f = check_outdated(&[pkg("1.0.0")], &OfflineRegistry).unwrap();
        assert!(f.is_empty());
    }
}
