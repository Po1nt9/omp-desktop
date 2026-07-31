# Credential Management Guide Doc — Design (AC-12.3)

**Date:** 2026-07-31 · **Status:** approved (user-absent; recommended defaults adopted and recorded here)
**Authority:** [Master Design §8.1/§8.2](./2026-07-28-omp-desktop-design.md) · [§18.2 pass conditions 6 & 12](./2026-07-28-omp-desktop-design.md) · [credential-migration spec](./2026-07-31-credential-migration-design.md) · [1.0 acceptance matrix AC-12.3](../../release/1.0-acceptance-matrix.md) · [security-audit-checklist SA-C](../../release/security-audit-checklist.md)

## 1. Problem

AC-12.3 is the last real FAIL in the 1.0 acceptance matrix:

> Credential management guide (keychain, migration, no plaintext fallback) — Doc review — FAIL — No credential-management doc.

The behavior it must document **has already landed** (§8.2 credential migration + strict mode, commits `6a88b26..197a97a`, 2026-07-31): keychain is the only credential backend, plaintext fallback is gone, startup auto-migration runs the 6-step idempotent flow. The matrix row's original unblocker ("fix §6 first, then document") is resolved; what remains is writing the guide itself.

## 2. Decisions (user-absent defaults)

| ID | Decision | Rationale |
|----|----------|-----------|
| D1 | Standalone English guide at `docs/credential-management.md` | Matches docs/ convention (`desktop-auto-update.md`, `release/signing-requirements.md` are English); single-file doc-review evidence. Alternatives rejected: README-split fragments evidence + doubles bilingual burden; `docs/release/` is acceptance infrastructure, wrong audience. |
| D2 | **Honest boundaries section**: document only landed behavior; §8.1 components not yet implemented (auth-broker sole OAuth writer, `CredentialIndexStore`, `agent.db` reference migration, release-artifact secret scan — SA-C.2/C.4/C.6/C.7/C.8/C.9 PENDING) are listed as roadmap, never as present behavior. | The matrix FAIL note warns docs must not claim guarantees the code doesn't meet; the symmetric risk is documenting aspirational architecture as fact. |
| D3 | README.md + README_EN.md each gain exactly one link line to the new guide | Bilingual README sync rule; discoverability without content duplication. |
| D4 | Matrix AC-12.3 FAIL→PASS with evidence; counts 40→41 PASS, grep-FAIL 2→1 (counts-table row only; real FAIL = 0); FAIL-list item struck with Resolved note | Same flip procedure as AC-10.9 (commit `989c445`). |
| D5 | No new automated tests — verification is doc review. Gates: `check:brand` / `check:provenance` / `check:legal` + link integrity + full cargo/vitest suites to catch accidental breakage | TC-M.12 relies on link-integrity check scripts; doc-only change must keep all existing gates green. |
| D6 | Guide language: English | docs/ directory convention (D1). |

## 3. Document structure (8 sections)

Style: status blockquote → tables → ASCII architecture diagram → file:line module index, mirroring `docs/desktop-auto-update.md`.

1. **Status blockquote** — strict mode live since 2026-07-31; keychain-only; migration auto-runs at startup.
2. **Where credentials live** — per-platform table (macOS Keychain / Windows Credential Manager / Linux Secret Service via the `keyring` crate); keyring service `com.omp-desktop.omp-desktop`, account naming `<ns>:<key>`; namespace isolation: `provider` (Agent Provider keys) vs `remote` (Remote IM channel credentials) per §8.1.
3. **What lands on disk** — `keychain:v1:<ns>:<key>` opaque references in `secrets.json` and `channel-secret-refs.json`; legacy plaintext `channel-secrets.json` securely deleted when its last field migrates; post-migration no credential material exists on disk. Legacy unprefixed keychain read fallback exists for the `provider` namespace only.
4. **Strict mode — no plaintext fallback** — store unavailable ⇒ save/refresh **blocked** with actionable i18n error `credentials.storeUnavailable` (3 locales; Linux recipe: start/unlock a Secret Service provider such as gnome-keyring, restart the app); loads fail closed once the migration ledger's `store_unavailable` flag is set; existing credentials are never touched on error paths.
5. **Migration (§8.2)** — the 6 steps (dry run → copy → readback with constant-time compare → reference commit → tombstone `__tombstoned_v1__` → cleanup/secure delete); idempotent, recorded in a non-sensitive migration ledger; auto-run at startup (`run_startup_migration`); store outage persists Safe Mode flag and defers; failures roll back uncommitted references, never reverse-copy plaintext; second run is a no-op.
6. **Operating guide** — entering/rotating/clearing Provider keys (Settings → Providers, `secretsSet`); Remote IM channel credentials (stored under the `remote` namespace, e.g. `keychain:v1:remote:<instance>:bot_token`); Linux Secret Service unlock recipe; how to inspect references on disk; what a re-migration does (nothing, unless a legacy source reappears).
7. **Security properties & honest boundaries** — guaranteed today (OS-store-only secrets, namespace isolation, no plaintext fallback, constant-time readback, tombstone-no-loss cleanup, metadata-only logging per SA-L.1) vs roadmap (auth-broker as sole OAuth refresh writer, `CredentialIndexStore`, `agent.db` opaque references, CLI naming unification, release-artifact scan — each with its SA-C id).
8. **File/module index** — `src-tauri/src/secrets/{mod,store,migration}.rs`, `remote_im/config.rs`, `lib.rs` startup wiring, i18n keys, with one-line descriptions and test counts.

## 4. Matrix & doc updates

- `docs/release/1.0-acceptance-matrix.md`: AC-12.3 row FAIL→PASS (evidence: guide path + section-to-requirement mapping + strict-mode/migration test counts); FAIL-list item struck "Resolved 2026-07-31"; counts table PASS 40→41, FAIL 2→1 (grep口径; 真实 FAIL 项归零). Check AC-12.3 references elsewhere in the matrix for stale wording (cf. line ~381 audit summary).
- `README.md` / `README_EN.md`: one link line each (D3).
- `CHANGELOG.md`: entry under `## [0.3.1-nightly]` `### Added / 新增` (bilingual, matching existing entries).

## 5. Non-goals

- No code changes, no new tests, no i18n key changes (D5).
- Not documenting AC-12.2/12.4/12.5/12.6/12.7 (separate BLOCKED doc work packages).
- No Safe Mode UI work; the doc describes existing fail-closed behavior only.
- No `docs/README.md` index creation (none exists; out of scope).

## 6. Acceptance

- `docs/credential-management.md` exists, covers keychain + migration + no-plaintext-fallback per the AC-12.3 row, and every behavior claim is backed by a file:line reference or test.
- AC-12.3 row flips to PASS; counts 41/16/100/1 (grep); real FAIL count = 0.
- All gates green: cargo 506+1, vitest 843, typecheck, check:i18n, check:brand, check:provenance, check:legal.
- Both READMEs link the guide; CHANGELOG entry added.

## 7. Self-review

- Placeholder scan: none — all sections concrete.
- Consistency: D2 honest-boundaries rule applied to sections 3/7 (roadmap items quarantined to §7 of the guide); counts match the AC-10.9 post-flip state (40/16/100/2) + one flip.
- Scope: single plan, 3 tasks (doc / matrix+README+CHANGELOG / gates+commit).
- Ambiguity: "securely deleted" = `std::fs::remove_file` after last-field cleanup (`secrets/migration.rs` channel adapter) — the guide states exactly that, no shred claim.
