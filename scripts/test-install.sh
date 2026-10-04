#!/usr/bin/env bash
#
# Tests for scripts/install.sh. Every external command the installer relies on
# is replaced with a stub, so nothing is downloaded, cloned, or built. Run it
# from anywhere:
#
#   ./scripts/test-install.sh

set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
installer="$here/install.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

pass=0
fail=0

ok() {
  pass=$((pass + 1))
  printf 'ok   - %s\n' "$1"
}

no() {
  fail=$((fail + 1))
  printf 'FAIL - %s\n' "$1"
  printf '%s\n' "$2" | sed 's/^/       /'
}

add_stub() {
  printf '%s\n' "$2" >"$mock_dir/$1"
  chmod +x "$mock_dir/$1"
}

# Fresh stub directory. uname reports Darwin so each test starts out "on macOS";
# individual tests override the piece they exercise.
new_mocks() {
  mock_dir="$tmp/mocks-$1"
  mkdir -p "$mock_dir"
  # Variables such as MOCK_UNAME are read when the stub runs, so the single
  # quotes are intentional and the bodies must stay unexpanded here.
  # shellcheck disable=SC2016
  add_stub uname '#!/bin/sh
printf "%s\n" "${MOCK_UNAME:-Darwin}"'
  # shellcheck disable=SC2016
  add_stub xcode-select '#!/bin/sh
exit "${MOCK_XCODE_EXIT:-0}"'
  add_stub cargo '#!/bin/sh
exit 0'
  add_stub rustc '#!/bin/sh
printf "rustc 1.94.0 (000000000 2026-01-01)\n"'
  add_stub node '#!/bin/sh
printf "v22.0.0\n"'
  add_stub hdiutil '#!/bin/sh
exit 0'
}

# npm stub: records the call, makes the build look like it produced the app
# bundle, and can fail the first build to exercise the DMG retry.
add_npm_stub() {
  # shellcheck disable=SC2016
  add_stub npm '#!/bin/sh
printf "npm %s\n" "$*" >>"${MOCK_LOG:?}"
if [ "$1" = "run" ]; then
  mkdir -p "${MOCK_BUNDLE:?}/macos/Wahana.app"
  if [ -n "${MOCK_FAIL_FIRST_BUILD:-}" ]; then
    count=0
    [ -f "${MOCK_STATE:?}" ] && count=$(cat "${MOCK_STATE:?}")
    count=$((count + 1))
    printf "%s\n" "$count" >"${MOCK_STATE:?}"
    if [ "$count" -eq 1 ]; then
      printf "simulated DMG failure\n" >&2
      exit 1
    fi
  fi
fi
exit 0'
}

add_git_stub() {
  # shellcheck disable=SC2016
  add_stub git '#!/bin/sh
last=""
for arg in "$@"; do last="$arg"; done
case "$1" in
  clone) mkdir -p "$last" ;;
esac
exit 0'
}

fake_checkout() {
  mkdir -p "$1/scripts" "$1/src-tauri"
  cp "$installer" "$1/scripts/install.sh"
  : >"$1/src-tauri/tauri.conf.json"
}

# --- tests ------------------------------------------------------------------

new_mocks non-darwin
out="$(MOCK_UNAME=Linux PATH="$mock_dir:$PATH" bash "$installer" 2>&1)"
rc=$?
if [ "$rc" -ne 0 ] && printf '%s\n' "$out" | grep -q 'macOS app'; then
  ok 'refuses to run on a non-macOS host'
else
  no 'refuses to run on a non-macOS host' "$out"
fi

new_mocks no-xcode
out="$(MOCK_XCODE_EXIT=1 PATH="$mock_dir:$PATH" bash "$installer" 2>&1)"
rc=$?
if [ "$rc" -ne 0 ] && printf '%s\n' "$out" | grep -q 'Xcode Command Line Tools'; then
  ok 'fails with a clear message when Xcode CLT are missing'
else
  no 'fails with a clear message when Xcode CLT are missing' "$out"
fi

new_mocks happy
add_npm_stub
repo="$tmp/happy-checkout"
fake_checkout "$repo"
log="$tmp/happy.log"
: >"$log"
out="$(
  MOCK_LOG="$log" \
    MOCK_BUNDLE="$repo/src-tauri/target/release/bundle" \
    PATH="$mock_dir:$PATH" \
    bash "$repo/scripts/install.sh" 2>&1
)"
rc=$?
if [ "$rc" -eq 0 ] &&
  printf '%s\n' "$out" | grep -q 'Rust found' &&
  printf '%s\n' "$out" | grep -q 'Node found' &&
  grep -q '^npm ci$' "$log" &&
  grep -q 'npm run tauri build' "$log" &&
  [ -d "$repo/src-tauri/target/release/bundle/macos/Wahana.app" ]; then
  ok 'builds from a checkout without installing a toolchain'
else
  no 'builds from a checkout without installing a toolchain' "$out"
fi

new_mocks retry
add_npm_stub
repo="$tmp/retry-checkout"
fake_checkout "$repo"
log="$tmp/retry.log"
: >"$log"
out="$(
  MOCK_LOG="$log" \
    MOCK_BUNDLE="$repo/src-tauri/target/release/bundle" \
    MOCK_FAIL_FIRST_BUILD=1 \
    MOCK_STATE="$tmp/retry.count" \
    PATH="$mock_dir:$PATH" \
    bash "$repo/scripts/install.sh" 2>&1
)"
rc=$?
if [ "$rc" -eq 0 ] &&
  printf '%s\n' "$out" | grep -q 'retrying (1/3)' &&
  [ "$(grep -c 'npm run tauri build' "$log")" -ge 2 ]; then
  ok 'retries the DMG step after a failed first build'
else
  no 'retries the DMG step after a failed first build' "$out"
fi

new_mocks standalone
add_npm_stub
add_git_stub
clone="$tmp/standalone-clone"
log="$tmp/standalone.log"
: >"$log"
out="$(
  MOCK_LOG="$log" \
    MOCK_BUNDLE="$clone/src-tauri/target/release/bundle" \
    WAHANA_DIR="$clone" \
    PATH="$mock_dir:$PATH" \
    bash -c "$(cat "$installer")" 2>&1
)"
rc=$?
if [ "$rc" -eq 0 ] &&
  printf '%s\n' "$out" | grep -q 'Cloning Wahana' &&
  [ -d "$clone/src-tauri/target/release/bundle/macos/Wahana.app" ]; then
  ok 'clones and builds when run standalone (curl | bash)'
else
  no 'clones and builds when run standalone (curl | bash)' "$out"
fi

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
