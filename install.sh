#!/bin/sh
# Allem single-command installer.
#   curl -fsSL https://allem.sh/install | sh
# Detects OS/arch, downloads the matching static `allem` binary, drops it on PATH.
# Zero toolchain required. (Windows users: use install.ps1.)
set -eu

REPO="walkowicz19/allem"
BIN="allem"
INSTALL_DIR="${ALLEM_INSTALL_DIR:-$HOME/.local/bin}"

os() {
  case "$(uname -s)" in
    Linux) echo linux ;;
    Darwin) echo macos ;;
    *) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
  esac
}

arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo x86_64 ;;
    arm64|aarch64) echo aarch64 ;;
    *) echo "unsupported arch: $(uname -m)" >&2; exit 1 ;;
  esac
}

main() {
  target="$(os)-$(arch)"
  url="https://github.com/${REPO}/releases/latest/download/${BIN}-${target}.tar.gz"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  echo "Downloading ${BIN} (${target})..."
  curl -fsSL "$url" -o "$tmp/${BIN}.tar.gz"
  tar -xzf "$tmp/${BIN}.tar.gz" -C "$tmp"

  mkdir -p "$INSTALL_DIR"
  install -m 0755 "$tmp/${BIN}" "$INSTALL_DIR/${BIN}"

  echo "Installed ${BIN} to ${INSTALL_DIR}/${BIN}"
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "Add ${INSTALL_DIR} to your PATH to use '${BIN}'." ;;
  esac
}

main "$@"
