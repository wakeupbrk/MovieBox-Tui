#!/usr/bin/env bash
# Install MovieBox-TUI (wakeupbrk fork) via cargo.
# Usage: curl -fsSL https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/install.sh | bash
set -euo pipefail

REPO="https://github.com/wakeupbrk/MovieBox-Tui.git"
BIN_NAME="moviebox-tui"

log_info()    { echo -e "\033[0;34m==>\033[0m $1"; }
log_success() { echo -e "\033[0;32m==>\033[0m $1"; }
log_warn()    { echo -e "\033[1;33mWARN:\033[0m $1"; }
log_err()     { echo -e "\033[0;31mERROR:\033[0m $1" >&2; exit 1; }

# Ensure cargo is available
if ! command -v cargo >/dev/null 2>&1; then
  log_warn "Rust/cargo not found. Installing rustup (interactive)…"
  if ! command -v curl >/dev/null 2>&1; then
    log_err "curl is required. Install curl, then re-run this script."
  fi
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

# Ensure cargo bin is on PATH for this shell
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
  log_err "cargo still not on PATH. Open a new terminal or: source \"\$HOME/.cargo/env\""
fi

log_info "Installing MovieBox-TUI from $REPO …"
cargo install --git "$REPO" --locked --force

if ! command -v "$BIN_NAME" >/dev/null 2>&1; then
  log_warn "Binary installed but not on PATH."
  log_warn "Add this to your shell config: export PATH=\"\$HOME/.cargo/bin:\$PATH\""
  log_success "Try: $HOME/.cargo/bin/$BIN_NAME"
  exit 0
fi

VERSION=$("$BIN_NAME" --version 2>/dev/null || echo "installed")
log_success "Done. $VERSION"
echo ""
echo "  Run:     $BIN_NAME"
echo "  Sources: Ctrl+P   Continue: Ctrl+W   Library: Ctrl+Z   Help: ?"
echo "  Repo:    https://github.com/wakeupbrk/MovieBox-Tui"
echo ""

if ! command -v mpv >/dev/null 2>&1 && ! command -v iina >/dev/null 2>&1 && ! command -v vlc >/dev/null 2>&1; then
  log_warn "No video player detected. Install mpv (recommended):"
  echo "         brew install mpv    # macOS"
  echo "         sudo apt install mpv  # Debian/Ubuntu"
fi
