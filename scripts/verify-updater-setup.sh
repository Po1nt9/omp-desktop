#!/usr/bin/env bash
# Verify desktop auto-update production prerequisites (local / CI).
# Does not print secret values. Exit 0 when checks that can run pass.
#
# Usage:
#   ./scripts/verify-updater-setup.sh
#   ./scripts/verify-updater-setup.sh --fetch-latest   # also HTTP-get latest.json
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FETCH_LATEST=0
for arg in "$@"; do
  case "$arg" in
    --fetch-latest) FETCH_LATEST=1 ;;
    -h|--help)
      sed -n '1,12p' "$0"
      exit 0
      ;;
  esac
done

ok=0
warn=0
fail=0

note() { printf '  · %s\n' "$*"; }
pass() { ok=$((ok + 1)); printf 'OK   %s\n' "$*"; }
warn() { warn=$((warn + 1)); printf 'WARN %s\n' "$*"; }
fail() { fail=$((fail + 1)); printf 'FAIL %s\n' "$*"; }

echo "== OMP Desktop updater setup check =="

# 1) Workflow references required secrets
WF=".github/workflows/release.yml"
if [[ -f "$WF" ]]; then
  if grep -q 'OMP_DESKTOP_UPDATER_PUBLIC_KEY' "$WF" \
    && grep -q 'TAURI_SIGNING_PRIVATE_KEY' "$WF"; then
    pass "release.yml references updater signing secrets"
  else
    fail "release.yml missing OMP_DESKTOP_UPDATER_* / TAURI_SIGNING_* wiring"
  fi
  if grep -q 'assemble-updater-manifest\|generate-latest-json\|omp-desktop-latest' "$WF"; then
    pass "release.yml publishes rolling updater assets"
  else
    warn "release.yml may not publish omp-desktop-latest / latest.json"
  fi
else
  fail "missing $WF"
fi

# 2) Local scripts present
for s in scripts/assemble-updater-manifest.sh scripts/generate-latest-json.sh scripts/build-release-config.mjs; do
  if [[ -f "$s" ]]; then
    pass "present $s"
  else
    fail "missing $s"
  fi
done

# 3) Docs
if [[ -f docs/desktop-auto-update.md ]]; then
  pass "docs/desktop-auto-update.md present"
else
  fail "missing docs/desktop-auto-update.md"
fi

# 4) Env presence (names only — never print values)
has_pub=0
has_priv=0
has_ep=0
[[ -n "${OMP_DESKTOP_UPDATER_PUBLIC_KEY:-}" ]] && has_pub=1
[[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] && has_priv=1
[[ -n "${OMP_DESKTOP_UPDATER_ENDPOINT:-}" ]] && has_ep=1

if [[ $has_pub -eq 1 && $has_priv -eq 1 ]]; then
  pass "local env has OMP_DESKTOP_UPDATER_PUBLIC_KEY + TAURI_SIGNING_PRIVATE_KEY"
else
  warn "local env missing signing keys (expected on maintainer machines / CI only)"
  note "Generate: pnpm tauri signer generate -w ~/.tauri/omp-desktop.key"
fi
if [[ $has_ep -eq 1 ]]; then
  pass "local env has OMP_DESKTOP_UPDATER_ENDPOINT"
else
  note "OMP_DESKTOP_UPDATER_ENDPOINT optional locally; CI sets it for release builds"
fi

# 5) Optional live latest.json
REPO="${GITHUB_REPOSITORY:-Po1nt9/omp-desktop}"
LATEST_URL="https://github.com/${REPO}/releases/download/omp-desktop-latest/latest.json"
if [[ $FETCH_LATEST -eq 1 ]]; then
  if command -v curl >/dev/null 2>&1; then
    code=$(curl -sS -o /tmp/omp-desktop-latest.json -w '%{http_code}' -L "$LATEST_URL" || true)
    if [[ "$code" == "200" ]]; then
      if command -v python3 >/dev/null 2>&1; then
        if python3 -c 'import json,sys; json.load(open("/tmp/omp-desktop-latest.json"))' 2>/dev/null; then
          pass "latest.json fetchable and valid JSON ($LATEST_URL)"
        else
          fail "latest.json HTTP 200 but not valid JSON"
        fi
      else
        pass "latest.json HTTP 200 ($LATEST_URL)"
      fi
    else
      warn "latest.json not fetchable (HTTP $code) — rolling release may not exist yet"
    fi
  else
    warn "curl not available; skip --fetch-latest"
  fi
else
  note "skip live latest.json (pass --fetch-latest to check $LATEST_URL)"
fi

echo
echo "Summary: ok=$ok warn=$warn fail=$fail"
if [[ $fail -gt 0 ]]; then
  exit 1
fi
exit 0
