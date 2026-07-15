#!/usr/bin/env bash
# Build and release the kioku CLI (docs/contributing.mdx "CLI Releases").
#
# Usage:
#   ./build.sh v0.1.2          # stable: bare tag, becomes GitHub "Latest"
#   ./build.sh v0.1.2-dev.1    # dev: cli/ tag namespace + --prerelease
#   ./build.sh v0.1.2 --build-only   # skip the gh release step
#
# Release description: extra args are passed through to `gh release create`,
# e.g. --notes "..." or --notes-file CHANGELOG.md. Default: --generate-notes
# (GitHub auto-generates from merged PRs since the previous release).
set -euo pipefail
cd "$(dirname "$0")"

VERSION="${1:?usage: ./build.sh v<X.Y.Z>[-dev.N] [--build-only | gh-release-args...]}"
shift
TARGET="x86_64-unknown-linux-gnu"
REPO="kioku-org/kioku"

# Sync the workspace version to the tag so the binary self-reports it —
# every crate inherits [workspace.package] version from this Cargo.toml.
BARE="${VERSION#v}"
CARGO_VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
if [ "$BARE" != "$CARGO_VERSION" ]; then
  sed -i "0,/^version = \"$CARGO_VERSION\"/s//version = \"$BARE\"/" Cargo.toml
  echo "synced Cargo.toml version: $CARGO_VERSION -> $BARE (remember to commit the bump)"
fi

cargo build --release -p kioku-cli

TARBALL="target/release/kioku-${VERSION}-${TARGET}.tar.gz"
tar -czf "$TARBALL" -C target/release kioku
echo "built $TARBALL"

[ "${1:-}" = "--build-only" ] && exit 0

# Description: caller-supplied gh args win; otherwise auto-generate notes
# (without a notes flag, gh release create goes interactive).
NOTES_ARGS=("$@")
[ ${#NOTES_ARGS[@]} -eq 0 ] && NOTES_ARGS=(--generate-notes)

# Prerelease suffix (anything after '-') => dev release: cli/ tag namespace +
# --prerelease so install.sh's releases/latest never resolves to it.
if [[ "$VERSION" == *-* ]]; then
  gh release create "cli/$VERSION" "$TARBALL" \
    --repo "$REPO" --title "kioku $VERSION" --prerelease "${NOTES_ARGS[@]}"
else
  gh release create "$VERSION" "$TARBALL" \
    --repo "$REPO" --title "kioku $VERSION" "${NOTES_ARGS[@]}"
fi
