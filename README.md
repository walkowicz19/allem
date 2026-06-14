# Allem

**Polyglot codebase & dependency intelligence — one command, and an MCP server.**

Allem is a deterministic static-analysis engine that works across **13 languages** and validates
your dependencies for being **outdated**, **vulnerable**, or **dangerous** (suspicious files,
injection, typosquats). It never bulk-fixes: every issue is a reviewable **candidate** so a human
or an LLM can confirm it vs. a false positive before anything changes.

It is **inspired by [`fallow`](https://github.com/fallow-rs/fallow)** — see [Credits](#credits).

## Highlights

- **Polyglot** — Python, Rust, Go, Ruby, Java, JavaScript, TypeScript, C, C++, C#, PHP, Bash
  (tree-sitter), plus a bespoke **COBOL** adapter. Adding a language is one `LangSpec`.
- **Dependency safety** — **outdated** (PyPI / crates.io registries), **vulnerable** (live
  [OSV.dev](https://osv.dev)), **dangerous** (unpinned wildcards, untrusted VCS/URL sources,
  typosquats), and **injection** (install-script markers).
- **Code intelligence** — cyclomatic complexity, long functions, AST-level **injection sinks**,
  **cross-file dead code**, **cross-file duplication** (copy-paste, comment-insensitive), and
  **syntax/parse errors**.
- **Triage-first** — verdicts persist in `.allem/triage.json`; false positives are excluded from
  CI gating; `fix` applies a **bounded, single-finding** change only.
- **Single binary** — one install command, zero toolchain. Also an **MCP server**.

## Run it

**No install, no clone, no toolchain — one command:**

```sh
npx allem analyze .
```

On first run this downloads a small prebuilt binary for your platform (Linux x64, macOS
x64/arm64, Windows x64) and caches it; later runs are instant. You can run any
command this way — `npx allem audit .`, `npx allem mcp`, etc.

### Other ways to install

```sh
# From source, straight from GitHub (needs a Rust toolchain):
cargo install --git https://github.com/walkowicz19/allem allem-cli

# Once published: prebuilt installer / crates.io
curl -fsSL https://allem.sh/install | sh
cargo install allem-cli
```

## Usage

```sh
allem analyze .                # human-readable report
allem analyze . --format json  # stable Finding[] contract (for CI / agents)
allem audit .                  # CI gate: non-zero exit at/above gate severity

# triage (never bulk-fixes)
allem confirm <finding-id>     # mark a real issue
allem ignore  <finding-id>     # dismiss a false positive (excluded from gating)
allem fix     <finding-id>     # apply ONE bounded fix (e.g. a dependency upgrade)
allem triage                   # show recorded verdicts

allem mcp                      # start the MCP server (stdio)
```

### Example

```text
$ npx allem analyze ./my-service
Allem report for ./my-service
  languages: python, go
  ecosystems: PyPI
  findings: 4 total (0 critical, 2 high, 1 medium, 0 low, 1 info)
  - [HIGH] requests 2.19.0 is affected by PYSEC-2018-28 (dep/PyPI/requests/PYSEC-2018-28)
  - [HIGH] dangerous call `os.system` (matches sink `os.system`) in python (inject/python/os.system:12)
  - [MED ] `flask` is outdated: 1.0.0 → 3.1.3 available (dep/outdated/PyPI/flask)
  - [INFO] `legacy_helper` (python) appears unused (deadcode/python/legacy_helper:8)
```

Each finding carries a stable `id`, severity, evidence, and a suggested action. Nothing is
changed until you explicitly `allem fix <id>` or dismiss it with `allem ignore <id>`.

## MCP server

`allem mcp` speaks newline-delimited **JSON-RPC 2.0** over stdio and exposes the same `Finding`
contract the CLI emits. Tools:

| Tool                    | Purpose                                                         |
| ----------------------- | --------------------------------------------------------------- |
| `analyze_repo`          | Full deterministic report (languages, ecosystems, all findings) |
| `list_dependency_risks` | Dependency findings, optionally filtered to a `min_severity`    |
| `explain_finding`       | Full evidence bundle for one finding id (for triage)            |
| `confirm_finding`       | Record a verdict: `confirmed` or `false_positive`               |
| `apply_fix`             | Apply exactly one bounded fix and mark it `fixed`               |

### Configuration

A project-scoped config is included at [`.mcp.json`](.mcp.json) (Claude Code picks it up
automatically). It uses `npx`, so there's nothing to install:

```json
{
  "mcpServers": {
    "allem": {
      "command": "npx",
      "args": ["-y", "allem", "mcp"]
    }
  }
}
```

For **Claude Desktop**, add the same `mcpServers` block to `claude_desktop_config.json`. If you
installed the native binary instead, use `"command": "allem", "args": ["mcp"]` (with an absolute
path if it isn't on `PATH`).

## Workspace

| Crate          | Responsibility                                                                 |
| -------------- | ------------------------------------------------------------------------------ |
| `allem-core`   | `Finding`/`Report` contract, adapter traits, config, triage store              |
| `allem-lang`   | Polyglot code intelligence (tree-sitter + COBOL): complexity, sinks, dead code |
| `allem-deps`   | Dependency intelligence: parsing, outdated, OSV, danger, bounded fixes         |
| `allem-engine` | Orchestrator — merges code + dependency intelligence, drives triage            |
| `allem-mcp`    | MCP server over stdio                                                          |
| `allem-cli`    | The `allem` binary                                                             |

## Development

```sh
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Best practices for this codebase are grounded throughout via the **klayer** MCP — see
[Best practices](#best-practices--klayer).

## Best practices · klayer

Allem is built using **[klayer](https://github.com/walkowicz19/klayer)** as its best-practices
layer. During implementation, klayer's `rust-dev`, `clean-code`, and `cybersecurity` knowledge
domains are consulted (via `recall`) before writing each module, and milestones are logged to its
episodic memory. The conventions it enforces here:

- `#![forbid(unsafe_code)]`; `cargo clippy --all-targets --all-features -- -D warnings` in CI.
- No `unwrap()`/`expect()` in production paths — enum errors via `thiserror` (libraries),
  application errors via `anyhow` (binary).
- KISS / DRY / YAGNI / ISP — many small adapters over one bloated engine.
- Supply-chain and injection-prevention guidance backing the dependency-safety checks.

> Use **klayer** for best practices: <https://github.com/walkowicz19/klayer>

## Credits

- **[fallow](https://github.com/fallow-rs/fallow)** — Allem's design is inspired by fallow, a
  Rust-native deterministic codebase-intelligence tool for TypeScript/JavaScript. Allem keeps its
  core philosophy (deterministic analyzer, structured truth for humans/CI/agents, MCP surface)
  and extends it to many languages and to dependency-safety triage. Credit and thanks to the
  fallow authors.
- **[klayer](https://github.com/walkowicz19/klayer)** — used as the best-practices / knowledge
  layer throughout development.
- **[OSV.dev](https://osv.dev)** and **[tree-sitter](https://tree-sitter.github.io/tree-sitter/)**
  — vulnerability data and multi-language parsing.

## License

MIT OR Apache-2.0
