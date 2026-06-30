#!/usr/bin/env sh
# Kioku CLI installer
# Usage: curl -fsSL https://kioku.chat/install.sh | sh
set -e

REPO="kioku-org/kioku"
BIN="kioku"
RELEASES="https://github.com/${REPO}/releases/latest/download"

# ── Detect OS ────────────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  os="linux" ;;
  Darwin) os="macos" ;;
  *)
    echo "error: unsupported OS: $OS" >&2
    echo "       Install from source: cargo install kioku-cli" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64)          arch="x86_64" ;;
  aarch64 | arm64) arch="aarch64" ;;
  *)
    echo "error: unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

case "${os}-${arch}" in
  linux-x86_64)   target="x86_64-unknown-linux-gnu" ;;
  linux-aarch64)  target="aarch64-unknown-linux-gnu" ;;
  macos-x86_64)   target="x86_64-apple-darwin" ;;
  macos-aarch64)  target="aarch64-apple-darwin" ;;
esac

TARBALL="${BIN}-${target}.tar.gz"
URL="${RELEASES}/${TARBALL}"

# ── Download ─────────────────────────────────────────────────────────────────
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading kioku for ${os}/${arch}..."
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$URL" -o "$TMP/$TARBALL"
elif command -v wget >/dev/null 2>&1; then
  wget -qO "$TMP/$TARBALL" "$URL"
else
  echo "error: curl or wget is required" >&2
  exit 1
fi

tar -xzf "$TMP/$TARBALL" -C "$TMP"

# ── Install ───────────────────────────────────────────────────────────────────
# Try /usr/local/bin first, fall back to ~/.local/bin without sudo
if [ -w "/usr/local/bin" ]; then
  INSTALL_DIR="/usr/local/bin"
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  INSTALL_DIR="/usr/local/bin"
  USE_SUDO=1
else
  INSTALL_DIR="${HOME}/.local/bin"
  mkdir -p "$INSTALL_DIR"
fi

if [ "${USE_SUDO:-0}" = "1" ]; then
  sudo install -m 755 "$TMP/$BIN" "$INSTALL_DIR/$BIN"
else
  install -m 755 "$TMP/$BIN" "$INSTALL_DIR/$BIN"
fi

echo ""
echo "  kioku $("$INSTALL_DIR/$BIN" --version 2>/dev/null || echo installed)"
echo "  installed to $INSTALL_DIR/$BIN"
echo ""

# ── PATH hint ─────────────────────────────────────────────────────────────────
case ":${PATH}:" in
  *:"$INSTALL_DIR":*) ;;
  *)
    echo "  Add to your shell profile:"
    echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo ""
    ;;
esac

echo "  Get started:"
echo "    kioku signin"
echo "    kioku --help"
