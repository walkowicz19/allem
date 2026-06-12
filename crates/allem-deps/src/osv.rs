//! Vulnerability lookup. `VulnSource` is a trait so the engine is testable offline with a
//! fake, and so we can swap data sources. The real impl queries OSV.dev — one API covering
//! PyPI, crates.io, npm, Go, Maven, RubyGems and 30+ ecosystems.

use crate::model::{Advisory, Package};
use allem_core::Result;

/// A source of vulnerability advisories for a concrete package version.
pub trait VulnSource: Send + Sync {
    fn query(&self, package: &Package) -> Result<Vec<Advisory>>;
}

/// Offline / disabled source: reports nothing. Used when `config.offline` is set or no
/// network is desired (keeps `analyze` deterministic and hermetic in tests).
pub struct OfflineSource;

impl VulnSource for OfflineSource {
    fn query(&self, _package: &Package) -> Result<Vec<Advisory>> {
        Ok(Vec::new())
    }
}

/// Live OSV.dev source. POSTs to `/v1/query`. Network failures are surfaced as empty
/// results rather than aborting the whole run — a missing lookup must not hide other
/// findings (fail-open for availability, never for severity).
pub struct OsvClient {
    endpoint: String,
}

impl Default for OsvClient {
    fn default() -> Self {
        OsvClient {
            endpoint: "https://api.osv.dev/v1/query".to_string(),
        }
    }
}

impl VulnSource for OsvClient {
    fn query(&self, package: &Package) -> Result<Vec<Advisory>> {
        let Some(version) = package.version.concrete() else {
            // No concrete version → cannot map to advisories precisely here.
            return Ok(Vec::new());
        };
        let body = serde_json::json!({
            "version": version,
            "package": { "name": package.name, "ecosystem": package.ecosystem }
        });

        let resp = match ureq::post(&self.endpoint).send_json(body) {
            Ok(r) => r,
            Err(_) => return Ok(Vec::new()), // fail-open on transport/HTTP errors
        };
        let value: serde_json::Value = match resp.into_json() {
            Ok(v) => v,
            Err(_) => return Ok(Vec::new()),
        };
        Ok(parse_osv_response(&value))
    }
}

/// Pure parser for an OSV `/v1/query` response — unit-testable without the network.
pub fn parse_osv_response(value: &serde_json::Value) -> Vec<Advisory> {
    let Some(vulns) = value.get("vulns").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    vulns
        .iter()
        .map(|v| Advisory {
            id: v
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or("UNKNOWN")
                .to_string(),
            summary: v
                .get("summary")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            fixed_in: first_fixed(v),
            severity: v
                .get("database_specific")
                .and_then(|d| d.get("severity"))
                .and_then(|s| s.as_str())
                .map(str::to_string),
        })
        .collect()
}

/// Extract the first `fixed` version from an OSV entry's affected ranges, if any.
fn first_fixed(vuln: &serde_json::Value) -> Option<String> {
    let affected = vuln.get("affected")?.as_array()?;
    for a in affected {
        let Some(ranges) = a.get("ranges").and_then(|r| r.as_array()) else {
            continue;
        };
        for r in ranges {
            let Some(events) = r.get("events").and_then(|e| e.as_array()) else {
                continue;
            };
            for e in events {
                if let Some(fixed) = e.get("fixed").and_then(|f| f.as_str()) {
                    return Some(fixed.to_string());
                }
            }
        }
    }
    None
}
