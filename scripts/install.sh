#!/usr/bin/env bash
# Build Wahana from source: installs Rust and Node when missing, then runs
# `npm run tauri build`.
#
# Usage, from a checkout:   ./scripts/install.sh
# Usage, standalone:        bash -c "$(curl -fsSL https://raw.githubusercontent.com/ashafizullah/wahana/main/scripts/install.sh)"
#
# Standalone runs clone the repository into $WAHANA_DIR (default ~/wahana),
# or update it with a fast-forward pull when it is already there.
set -euo pipefail

# Minimum Rust version, from the `rust-version` of the locked dependencies in
# src-tauri/Cargo.lock (whatsapp-rust / wacore 0.7.0 require 1.94).
MIN_RUST="1.94.0"
REPO_URL="https://github.com/ashafizullah/wahana.git"
WAHANA_DIR="${WAHANA_DIR:-$HOME/wahana}"

info() { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
fail() { printf '\033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || fail "Wahana is a macOS app; run this on macOS."
command -v curl >/dev/null || fail "curl is required."
xcode-select -p >/dev/null 2>&1 ||
  fail "Xcode Command Line Tools are required. Install them with: xcode-select --install"

# --- Source -----------------------------------------------------------------
# Run from the repository root: the checkout this script lives in, or a fresh
# clone when it was fetched with curl (BASH_SOURCE is empty then).
script_dir=""
if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi
if [[ -n "$script_dir" && -f "$script_dir/../src-tauri/tauri.conf.json" ]]; then
  cd "$script_dir/.."
elif [[ -d "$WAHANA_DIR/.git" ]]; then
  info "Updating existing checkout in $WAHANA_DIR..."
  git -C "$WAHANA_DIR" pull --ff-only
  cd "$WAHANA_DIR"
elif [[ -e "$WAHANA_DIR" ]]; then
  fail "$WAHANA_DIR exists but is not a git checkout; set WAHANA_DIR to another path."
else
  info "Cloning Wahana into $WAHANA_DIR..."
  git clone --depth 1 "$REPO_URL" "$WAHANA_DIR"
  cd "$WAHANA_DIR"
fi

# --- Rust -------------------------------------------------------------------
# A previous rustup install may not be on PATH in this shell yet.
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"

if command -v cargo >/dev/null && command -v rustc >/dev/null; then
  info "Rust found: $(rustc --version)"
  current="$(rustc --version | awk '{print $2}')"
  if [[ "$(printf '%s\n%s\n' "$MIN_RUST" "$current" | sort -V | head -n1)" != "$MIN_RUST" ]]; then
    command -v rustup >/dev/null || fail "Rust $current is older than $MIN_RUST; please upgrade it."
    info "Rust $current is older than $MIN_RUST, updating the stable toolchain..."
    rustup update stable
  fi
else
  info "Rust not found, installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
  info "Installed $(rustc --version)"
fi

# --- Node -------------------------------------------------------------------
# Installed through nvm (which also brings npm). Pin the same major version CI
# builds with (see .github/workflows/ci.yml) so a fresh Node release cannot
# change how the app builds.
NODE_VERSION="22"
NVM_VERSION="v0.40.8"
export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"

if command -v npm >/dev/null && command -v node >/dev/null; then
  info "Node found: $(node --version)"
else
  if [[ ! -s "$NVM_DIR/nvm.sh" ]]; then
    info "nvm not found, installing..."
    curl -fsSL "https://raw.githubusercontent.com/nvm-sh/nvm/$NVM_VERSION/install.sh" | bash
  fi
  # A previous nvm install is not on PATH in this shell yet.
  # shellcheck source=/dev/null
  source "$NVM_DIR/nvm.sh"
  info "Installing Node $NODE_VERSION via nvm..."
  nvm install "$NODE_VERSION"
  info "Installed $(node --version)"
fi

# --- Build ------------------------------------------------------------------
BUNDLE_DIR="$PWD/src-tauri/target/release/bundle"

# Eject disk images left mounted by an earlier build; a mounted image makes the
# DMG step fail with "hdiutil: couldn't unmount ... Resource busy". Only images
# built under target/ are detached, so a Wahana DMG the user opened on purpose
# is left mounted.
eject_stale_dmgs() {
  local dev
  for dev in $(hdiutil info | awk -v root="$PWD/src-tauri/target/" '
    /^image-path/ { sub(/^image-path[ \t]*:[ \t]*/, ""); hit = (index($0, root) == 1); next }
    hit && /^\/dev\/disk[0-9]+/ { d = $1; sub(/s[0-9]+$/, "", d); print d; hit = 0 }'); do
    hdiutil detach "$dev" -force -quiet || true
  done
  rm -f "$BUNDLE_DIR"/macos/rw.*.dmg
}

info "Installing frontend dependencies..."
npm ci

eject_stale_dmgs
info "Building Wahana (npm run tauri build)..."
if ! npm run tauri build; then
  # Tauri's bundled create-dmg script only retries `hdiutil detach` on exit
  # code 16, but a transient "Resource busy" (Finder/Spotlight touching the
  # fresh volume) exits with 1, so the DMG step fails intermittently. The app
  # itself is already built at this point; retry just the DMG step.
  [[ -d "$BUNDLE_DIR/macos/Wahana.app" ]] || fail "Build failed."
  for attempt in 1 2 3; do
    info "DMG bundling failed, retrying ($attempt/3)..."
    eject_stale_dmgs
    sleep 2
    npm run tauri build -- --bundles dmg && break
    (( attempt == 3 )) && fail "DMG bundling failed after 3 retries."
  done
fi
eject_stale_dmgs

info "Done. Build artifacts:"
ls -d "$BUNDLE_DIR"/macos/*.app "$BUNDLE_DIR"/dmg/*.dmg 2>/dev/null || true
