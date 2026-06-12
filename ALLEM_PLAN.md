# Allem — Solution Plan

> Polyglot codebase & dependency intelligence engine. Inspired by [`fallow-rs/fallow`](https://github.com/fallow-rs/fallow), extended to many languages and to **dependency safety triage**, delivered as a single-command CLI **and** an MCP server.

---

## 1. Vision

Fallow is a Rust-native, deterministic static-analysis engine for **TypeScript/JavaScript only**. It produces structured quality reports (dead code, dupes, complexity, risk, boundaries) for humans, CI, and AI agents, and exposes itself as an MCP server.

**Allem** keeps fallow's core philosophy — *deterministic, evidence-based, no AI inside the analyzer; the AI consumes the structured truth* — but changes three things:

1. **Polyglot.** First-class support for Python, Rust, Ruby, Java, COBOL, Go, JS/TS, and an extensible adapter system for "and much more."
2. **Dependency safety validation.** Beyond hygiene (unused/unlisted deps), Allem flags dependencies that are **outdated** or **dangerous** (known CVEs, suspicious files, install-script injection, typosquats, malware signals).
3. **Triage-first remediation.** Allem **never bulk-fixes**. Each finding is emitted as a discrete, reviewable candidate with evidence, so the human or LLM can confirm a real issue vs. a false positive before any change.

---

## 2. Differences from fallow (at a glance)

| Aspect | fallow | Allem |
|---|---|---|
| Languages | TS/JS | Python, Rust, Ruby, Java, COBOL, Go, TS/JS, + plugin adapters |
| Parser | Oxc (JS/TS only) | tree-sitter (300+ grammars, on-demand) + native manifest parsers |
| Dependency checks | hygiene (unused/unlisted/test-only) | hygiene **+ outdated + vulnerable + malicious/suspicious + injection** |
| Remediation | `actions[]` with `auto_fixable` | candidate findings, **one-at-a-time confirm**, no bulk auto-fix |
| Distribution | npm / npx / docker | single-command install (one script, no toolchain needed) + MCP |
| Audience | humans, CI, agents | same, with MCP as a primary surface |

---

## 3. Architecture

Layered, plugin-driven, deterministic core (clean-code: KISS / ISP — many small adapters over one bloated engine).

```
                       ┌─────────────────────────────────────────┐
                       │  Surfaces                                 │
                       │  • CLI (allem ...)   • MCP server         │
                       │  • CI formatters (SARIF/JSON/annotations) │
                       └───────────────┬───────────────────────────┘
                                       │  stable JSON contract (Finding[])
                       ┌───────────────▼───────────────────────────┐
                       │  Orchestrator / Report builder             │
                       │  scheduling, caching, scoring, gating      │
                       └───────┬───────────────────────┬────────────┘
              ┌────────────────▼──────┐      ┌──────────▼─────────────────┐
              │  Code Intelligence    │      │  Dependency Intelligence    │
              │  (per-language)       │      │  (per-ecosystem)            │
              │  • parse (tree-sitter)│      │  • manifest/lockfile parse  │
              │  • dead code / dupes  │      │  • outdated check           │
              │  • complexity / churn │      │  • vuln check (OSV)         │
              │  • boundaries         │      │  • danger/injection scan    │
              └───────────┬───────────┘      └──────────┬──────────────────┘
                          │                              │
              ┌───────────▼───────────┐      ┌───────────▼──────────────────┐
              │  LanguageAdapter trait │      │  EcosystemAdapter trait      │
              │  Py·Rs·Rb·Java·COBOL·Go│      │  pip·cargo·gem·maven·gomod·npm│
              └────────────────────────┘      └──────────────────────────────┘
```

### Core crates (Rust workspace)

- **`allem-core`** — Finding model, scoring, caching, config, error types (`thiserror` for the library, `anyhow`/`eyre` at binary boundaries — *never `unwrap()` in production paths*). `#![deny(unsafe_code)]` crate-wide.
- **`allem-lang`** — `LanguageAdapter` trait + tree-sitter-backed implementations. Grammars loaded on demand & cached (tree-sitter-language-pack-style) so install stays small.
- **`allem-deps`** — `EcosystemAdapter` trait: manifest/lockfile parsing, version resolution, OSV queries, danger heuristics.
- **`allem-mcp`** — MCP server exposing the same engine over stdio/HTTP.
- **`allem-cli`** — thin binary; argument parsing, output formatting, exit codes/gating.

Why Rust: matches fallow, gives a single static binary (key for single-command install), sub-second analysis, and easy cross-compilation. Adapters are *syntactic* (tree-sitter), so findings stay **deterministic** — no type-checker or language runtime required.

---

## 4. Polyglot code intelligence

A single `LanguageAdapter` trait keeps the orchestrator language-agnostic:

```rust
trait LanguageAdapter {
    fn id(&self) -> &'static str;            // "python", "cobol", ...
    fn matches(&self, path: &Path) -> bool;  // extensions / shebang / heuristics
    fn parse(&self, src: &str) -> SyntaxTree; // tree-sitter
    fn symbols(&self, tree: &SyntaxTree) -> Vec<Symbol>;   // defs/exports/imports
    fn metrics(&self, tree: &SyntaxTree) -> Metrics;       // complexity, size
}
```

**Launch languages:** Python, Rust, Ruby, Java, COBOL, Go, TS/JS. tree-sitter has maintained grammars for all of these (COBOL included), so "and much more" is a matter of registering additional grammars — no engine changes.

Shared, language-neutral analyses built on the symbol/metric output: dead code (unreferenced symbols/files), duplication, complexity hotspots (churn × difficulty), import/dependency graph, circular deps, and architecture-boundary checks. COBOL gets pragmatic adaptations (copybook resolution, paragraph/section-level reachability) rather than forcing a JS-shaped model onto it (clean-code: LSP — adapters honor the same contract without distorting it).

---

## 5. Dependency safety validation (the headline feature)

`EcosystemAdapter` parses each ecosystem's manifest + lockfile and produces a normalized package set:

| Ecosystem | Files |
|---|---|
| Python | `requirements*.txt`, `pyproject.toml`, `poetry.lock`, `Pipfile.lock` |
| Rust | `Cargo.toml`, `Cargo.lock` |
| Ruby | `Gemfile`, `Gemfile.lock` |
| Java | `pom.xml`, `build.gradle(.kts)` |
| Go | `go.mod`, `go.sum` |
| JS/TS | `package.json`, lockfiles |

Each package is run through four independent checks, every hit emitted as a discrete `Finding`:

1. **Outdated** — installed/pinned version vs. latest stable from the registry; classify patch/minor/major drift and EOL/yanked status.
2. **Vulnerable** — query **OSV.dev** (covers PyPI, crates.io, RubyGems, Maven, Go, npm, and 30+ ecosystems via one API) for known CVEs/advisories affecting the resolved version. Cached locally; offline mode falls back to a periodic OSV dump.
3. **Dangerous / suspicious files** — heuristic scan of package contents/metadata: install/build hooks (`postinstall`, `setup.py` exec, gem `extconf`, Gradle `doLast`), bundled binaries/minified blobs, obfuscated or base64-encoded payloads, network calls at install time, and metadata anomalies (recent maintainer change, very new package shadowing a popular name → **typosquat** scoring via edit distance to known-popular names).
4. **Injection vectors** — dynamic-execution / shell-out sinks reachable from dependency entry points (`eval`, `exec`, `system`, deserialization, dynamic `require`/`import`). Reuses the tree-sitter layer so the same detectors work across languages. Grounded in the cybersecurity rules (injection prevention, never-trust-input, supply-chain/APT defenses).

> Severity is a transparent, explainable score (each signal contributes a weighted, documented amount) — no opaque ML verdict, consistent with fallow's deterministic stance.

### Triage-first remediation (no bulk fix)

This is a deliberate design constraint. Allem **does not** apply sweeping fixes. Instead:

- Each finding is a self-contained candidate carrying **evidence** (file:line, the suspicious snippet, the advisory ID, the version delta) and a **suggested action** with a confidence level — never auto-applied.
- The CLI/MCP surface findings **one reviewable unit at a time**, so a human or LLM can mark each as **confirmed** or **false positive** before anything changes.
- A fix is only performed on explicit, per-finding confirmation (`allem fix <finding-id>`), bounded to that single change. This lets the LLM reason about each candidate and prevents a false positive from cascading into a broken tree.

```jsonc
// stable Finding contract (consumed identically by CLI, CI, MCP, and LLMs)
{
  "id": "dep/py/requests/CVE-2024-XXXX",
  "category": "dependency.vulnerable",
  "severity": "high",
  "package": { "ecosystem": "pypi", "name": "requests", "version": "2.19.0" },
  "evidence": { "advisory": "CVE-2024-XXXX", "fixed_in": "2.31.0",
                "locations": ["requirements.txt:12"] },
  "suggested_action": { "type": "upgrade", "to": "2.31.0", "confidence": "high" },
  "auto_applied": false,               // always false until confirmed
  "status": "candidate"                // candidate | confirmed | false_positive | fixed
}
```

---

## 6. MCP server

`allem mcp` starts the server (stdio for editors/agents, HTTP optional). It exposes the engine as structured truth so agents don't infer from grep (fallow's framing). Proposed tools:

- `analyze_repo(path, languages?, checks?)` → full `Finding[]` + scores.
- `inspect_target(path)` → combined evidence (dead code, dupes, complexity, deps) for one file/module.
- `list_dependency_risks(ecosystem?, severity?)` → outdated/vulnerable/dangerous candidates.
- `explain_finding(id)` → full evidence bundle for triage.
- `confirm_finding(id, verdict)` → mark confirmed / false_positive (drives the triage workflow).
- `apply_fix(id)` → bounded, single-finding fix; refuses bulk operations.

The MCP layer is a thin surface over `allem-core` — the same JSON contract the CLI emits — so behavior is identical across surfaces (DRY).

---

## 7. Single-command install

Goal: zero toolchain, one line, single static binary.

- **Primary:** `curl -fsSL https://allem.sh/install | sh` (and a PowerShell one-liner for Windows) — detects OS/arch, downloads the prebuilt static binary, drops it on `PATH`.
- **Also:** `brew install allem`, `cargo install allem`, `npm i -g allem` (wrapper), and a Docker image — meeting users where fallow's audience already is.
- **MCP registration:** `allem mcp install` writes the editor/agent MCP config automatically.
- **Zero-config by default:** auto-detects languages and ecosystems present; optional `.allemrc.jsonc` for overrides.

---

## 8. Tech stack (klayer-grounded)

- **Language:** Rust. `#![deny(unsafe_code)]`; `cargo clippy --all-targets --all-features` gated in CI; errors via `thiserror` (lib) + `anyhow` (bin), no `unwrap()`/`expect()` in production paths.
- **Parsing:** tree-sitter + on-demand grammar cache.
- **Vuln data:** OSV.dev API + cached offline dumps.
- **CLI:** `clap`. **Async/HTTP (OSV, registries):** `tokio` + `reqwest`.
- **Design discipline:** KISS / DRY / YAGNI / ISP — build the launch checks well, add ecosystems/languages only when actually needed.

---

## 9. Phased roadmap

- **M0 — Skeleton:** workspace, `Finding` contract, config, caching, CLI scaffold, CI with clippy/deny-unsafe.
- **M1 — Polyglot code intel:** `LanguageAdapter` + tree-sitter for the 7 launch languages; dead code, dupes, complexity, import graph.
- **M2 — Dependency hygiene + outdated:** `EcosystemAdapter` for all 6 ecosystems; outdated detection.
- **M3 — Safety validation:** OSV vuln checks, danger/suspicious-file heuristics, injection detectors, explainable severity scoring.
- **M4 — Triage workflow:** candidate lifecycle (confirm / false-positive / bounded fix), no bulk fix.
- **M5 — MCP server:** tools above + `allem mcp install`.
- **M6 — Distribution:** single-command installers, prebuilt binaries, Docker, CI formatters (SARIF/annotations).
- **M7 — "much more":** additional language/ecosystem adapters as plugins.

---

## 10. Using klayer throughout implementation

Per project requirement, klayer MCP is the source of best practices during build:

- **Before writing each module:** `recall(domain, query)` against `clean-code`, `rust-dev`, `cybersecurity` (e.g. recall injection/supply-chain rules before the danger scanner; recall Rust error-handling rules before the core).
- **Web lookups:** always via klayer `search_web` (OSV ecosystem coverage, tree-sitter grammar availability, registry APIs).
- **Codebase questions:** `index_codebase` then `search_code` so grounding persists across sessions.
- **Capture decisions:** `remember()` for confirmed facts, `propose()` for candidate conventions, `log_episode()` for milestones.
```
