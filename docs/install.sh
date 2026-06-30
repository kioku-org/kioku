#!/usr/bin/env sh
# Kioku CLI installer
# Usage: curl -fsSL https://kioku.chat/install.sh | sh
set -e

REPO="kioku-org/kioku"
BIN="kioku"
RELEASES="https://github.com/${REPO}/releases/latest/download"

# ── Colors (only when stdout is a tty) ───────────────────────────────────────
if [ -t 1 ]; then
  BOLD='\033[1m'; DIM='\033[2m'; RESET='\033[0m'
  RED='\033[31m'; GREEN='\033[32m'; CYAN='\033[36m'; WHITE='\033[97m'
else
  BOLD=''; DIM=''; RESET=''; RED=''; GREEN=''; CYAN=''; WHITE=''
fi

print() { printf "${BOLD}${WHITE}%s${RESET}\n" "$1"; }
ok()    { printf "  ${GREEN}✓${RESET}  %s\n" "$1"; }
fail()  { printf "  ${RED}✗${RESET}  %s\n" "$1" >&2; exit 1; }
dim()   { printf "${DIM}%s${RESET}\n" "$1"; }

# ── Spinner ───────────────────────────────────────────────────────────────────
# Animates while a background PID is running; falls back to a plain message
# when stdout is not a tty (piped, redirected, CI, etc.).
# Usage: spinner <pid> <message>
spinner() {
  pid=$1; msg=$2; i=0
  if [ ! -t 1 ]; then
    printf "  %s...\n" "$msg"
    return
  fi
  while kill -0 "$pid" 2>/dev/null; do
    case $((i % 10)) in
      0) f='⠋' ;; 1) f='⠙' ;; 2) f='⠹' ;; 3) f='⠸' ;; 4) f='⠼' ;;
      5) f='⠴' ;; 6) f='⠦' ;; 7) f='⠧' ;; 8) f='⠇' ;; 9) f='⠏' ;;
    esac
    printf "\r  ${CYAN}${BOLD}%s${RESET}  %s${DIM}...${RESET}" "$f" "$msg"
    sleep 0.08
    i=$((i + 1))
  done
  printf "\r\033[K"
}

# ── Header ────────────────────────────────────────────────────────────────────
printf "\n"
printf "  ${BOLD}${WHITE}𓄿  kioku${RESET}\n"
printf "  ${DIM}context infrastructure${RESET}\n"
printf "\n"

# ── Detect platform ───────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  os="linux" ;;
  Darwin) os="macos" ;;
  *) fail "unsupported OS: $OS  (install from source: cargo install kioku-cli)" ;;
esac

case "$ARCH" in
  x86_64)          arch="x86_64" ;;
  aarch64 | arm64) arch="aarch64" ;;
  *) fail "unsupported architecture: $ARCH" ;;
esac

case "${os}-${arch}" in
  linux-x86_64)   target="x86_64-unknown-linux-gnu" ;;
  linux-aarch64)  target="aarch64-unknown-linux-gnu" ;;
  macos-x86_64)   target="x86_64-apple-darwin" ;;
  macos-aarch64)  target="aarch64-apple-darwin" ;;
esac

dim "  platform: ${os}/${arch}"
printf "\n"

# ── Download ──────────────────────────────────────────────────────────────────
TARBALL="${BIN}-${target}.tar.gz"
URL="${RELEASES}/${TARBALL}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$URL" -o "$TMP/$TARBALL" &
elif command -v wget >/dev/null 2>&1; then
  wget -qO "$TMP/$TARBALL" "$URL" &
else
  fail "curl or wget is required"
fi

dl_pid=$!
spinner "$dl_pid" "Downloading"
wait "$dl_pid" || fail "Download failed — check your connection or try again"
ok "Downloaded"

# ── Extract ───────────────────────────────────────────────────────────────────
tar -xzf "$TMP/$TARBALL" -C "$TMP" &
spinner "$!" "Extracting"
wait "$!" || fail "Failed to extract archive"
ok "Extracted"

# ── Install ───────────────────────────────────────────────────────────────────
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
ok "Installed to ${INSTALL_DIR}/${BIN}"

# ── Done ─────────────────────────────────────────────────────────────────────
VERSION="$("$INSTALL_DIR/$BIN" --version 2>/dev/null || echo "kioku")"
printf "\n"
printf "  ${BOLD}${GREEN}${VERSION} is ready${RESET}\n"
printf "\n"

case ":${PATH}:" in
  *:"$INSTALL_DIR":*)
    ;;
  *)
    printf "  ${BOLD}Add to PATH:${RESET}\n"
    printf "  ${DIM}export PATH=\"${INSTALL_DIR}:\$PATH\"${RESET}\n"
    printf "\n"
    ;;
esac

printf "  ${DIM}kioku signin${RESET}\n"
printf "  ${DIM}kioku --help${RESET}\n"
printf "\n"
