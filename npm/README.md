# allem (npm wrapper)

Run **[Allem](https://github.com/walkowicz19/allem)** — polyglot codebase & dependency intelligence —
with no install and no Rust toolchain:

```sh
npx allem analyze .
```

On first run this downloads a small prebuilt binary for your platform from Allem's GitHub
Releases and caches it; subsequent runs are instant. The native CLI does all the work — this
package is just a thin launcher.

Supported platforms: Linux x64, macOS x64/arm64, Windows x64. On other platforms, build from
source: `cargo install --git https://github.com/walkowicz19/allem allem-cli`.

See the [main README](https://github.com/walkowicz19/allem) for full usage and the MCP server.

## License

MIT OR Apache-2.0
