#!/usr/bin/env bash
# OMP Desktop — one-line installer for macOS & Linux.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Po1nt9/omp-desktop/main/scripts/install.sh | bash
#   wget -qO- https://raw.githubusercontent.com/Po1nt9/omp-desktop/main/scripts/install.sh | bash
#
# What it does:
#   macOS  → downloads the .app.tar.gz updater archive, extracts to /Applications,
#            clears the Gatekeeper quarantine flag (xattr -cr).
#   Linux  → downloads the AppImage, makes it executable, installs to ~/.local/bin.
#
# The script is idempotent — safe to re-run for upgrades.

set -euo pipefail

REPO="Po1nt9/omp-desktop"
ROLLING_TAG="omp-desktop-latest"
API_BASE="https://api.github.com/repos/${REPO}/releases/tags/${ROLLING_TAG}"

# ── helpers ──────────────────────────────────────────────────────────────────

info()  { printf '\033[1;34m▶ %s\033[0m\n' "$*"; }
ok()    { printf '\033[1;32m✔ %s\033[0m\n' "$*"; }
err()   { printf '\033[1;31m✖ %s\033[0m\n' "$*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || err "'$1' is required but not found. Install it first."
}

fetch() {
  # fetch <url> — stdout, follows redirects
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- "$1"
  else
    err "Neither curl nor wget found. Install one of them first."
  fi
}

download() {
  # download <url> <dest>
  if command -v curl >/dev/null 2>&1; then
    curl -fSL --progress-bar -o "$2" "$1"
  else
    wget -q --show-progress -O "$2" "$1"
  fi
}

# ── detect platform ─────────────────────────────────────────────────────────

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) PLATFORM="macos" ;;
  Linux)  PLATFORM="linux" ;;
  *)      err "Unsupported OS: $OS (only macOS and Linux are supported by this script)" ;;
esac

case "$ARCH" in
  arm64|aarch64) ARCH_TAG="aarch64" ;;
  x86_64|amd64)  ARCH_TAG="x64" ;;
  *)             err "Unsupported architecture: $ARCH" ;;
esac

info "Detected: ${PLATFORM} / ${ARCH_TAG}"

# ── resolve download URL ────────────────────────────────────────────────────

info "Fetching latest release info from GitHub…"
RELEASE_JSON="$(fetch "$API_BASE")" || err "Failed to query GitHub API for ${ROLLING_TAG}"

if [[ "$PLATFORM" == "macos" ]]; then
  ASSET_NAME="OMP.Desktop_${ARCH_TAG}.app.tar.gz"
else
  # Linux: AppImage (universal x86_64; ARM not built yet)
  ASSET_NAME="OMP.Desktop_0.3.1-nightly_amd64.AppImage"
fi

DOWNLOAD_URL="$(printf '%s' "$RELEASE_JSON" \
  | grep -o "\"browser_download_url\": *\"[^\"]*${ASSET_NAME}\"" \
  | head -1 \
  | sed 's/.*: *"//; s/"//')"

[[ -n "$DOWNLOAD_URL" ]] || err "Asset '${ASSET_NAME}' not found in release ${ROLLING_TAG}"
info "Download URL: ${DOWNLOAD_URL}"

# ── install ──────────────────────────────────────────────────────────────────

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

if [[ "$PLATFORM" == "macos" ]]; then
  DEST="/Applications/OMP Desktop.app"
  info "Downloading macOS app archive…"
  download "$DOWNLOAD_URL" "$TMPDIR/app.tar.gz"

  info "Extracting to /Applications…"
  tar -xzf "$TMPDIR/app.tar.gz" -C "$TMPDIR"

  # The archive contains "OMP Desktop.app" at the top level.
  if [[ -d "$DEST" ]]; then
    info "Removing previous installation…"
    rm -rf "$DEST"
  fi
  mv "$TMPDIR/OMP Desktop.app" "$DEST"

  # Clear Gatekeeper quarantine so the app launches without the
  # "unidentified developer" dialog (no Apple Developer ID cert yet).
  info "Clearing macOS quarantine flag…"
  xattr -cr "$DEST" 2>/dev/null || true

  ok "Installed to ${DEST}"
  ok "Launch: open -a 'OMP Desktop'"

else
  # ── Linux ──
  INSTALL_DIR="${HOME}/.local/bin"
  mkdir -p "$INSTALL_DIR"
  DEST="${INSTALL_DIR}/omp-desktop"

  info "Downloading AppImage…"
  download "$DOWNLOAD_URL" "$DEST"
  chmod +x "$DEST"

  ok "Installed to ${DEST}"

  if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
    info "NOTE: ${INSTALL_DIR} is not in your PATH."
    info "Add it:  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
  fi

  ok "Launch: omp-desktop"
fi

ok "Done! 🎉"
