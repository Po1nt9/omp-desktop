# Credential Management Guide (AC-12.3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Author `docs/credential-management.md` against the landed §8.1/§8.2 behavior and flip AC-12.3 (the last real FAIL) to PASS.

**Architecture:** Documentation-only work package — no code, no new tests, no i18n changes. One English guide in `docs/` (style of `docs/desktop-auto-update.md`), matrix flip + README bilingual links + CHANGELOG entry, full gate run at the end.

**Tech Stack:** Markdown docs; acceptance matrix; README.md (Chinese) / README_EN.md (English).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-31-credential-management-doc-design.md` (D1-D6).
- **Honesty rule (D2):** document only landed behavior; unimplemented §8.1 components go in the roadmap subsection, never presented as current behavior.
- Doc language English (docs/ convention); README.md is Chinese-primary, README_EN.md English — both get exactly one link row (D3).
- No `window.confirm/prompt/alert`; no code changes at all in this package.
- No new dependencies. No secret values anywhere — env/file names only.
- Every Bash call starts with `cd /Users/po1nt9/Github/grok-app-main` (cwd resets between calls).
- Known flake: `store::tests::ensure_general_project_is_idempotent_and_not_removable` intermittent sandbox failure — re-run to confirm; documented non-product-bug (`docs/release/test-coverage-audit.md`).
- Commit per task, English commit messages with `(AC-12.3)` tag.
- Do not commit `tauri.release.conf.json` local diffs or any file outside the task list.

---

### Task 1: Write `docs/credential-management.md`

**Files:**
- Create: `docs/credential-management.md`

**Interfaces:**
- Consumes: landed behavior in `src-tauri/src/secrets/{mod,store,migration}.rs`, `src-tauri/src/remote_im/config.rs`, `src-tauri/src/lib.rs` (`run_startup_migration`), i18n keys `credentials.storeUnavailable` / `credentials.storeError`.
- Produces: the doc whose path Task 2 links from matrix + READMEs + CHANGELOG.

- [ ] **Step 1: Write the doc with this exact content**

````markdown
# Credential management

OMP Desktop stores all credential material in the **operating system's secure
store** — macOS Keychain, Windows Credential Manager, or a Linux Secret
Service provider. Strict mode (live since 2026-07-31) makes the OS store the
**only** credential backend: there is no plaintext fallback. A six-step
idempotent migration (master design §8.2) runs automatically at startup and
moves any legacy on-disk secrets into the OS store.

## Where credentials live

| Platform | Secure store |
|----------|--------------|
| macOS | Keychain |
| Windows | Credential Manager |
| Linux | Secret Service (e.g. gnome-keyring, KWallet) |

All access goes through a single Rust helper — the `SecretStore` trait in
`src-tauri/src/secrets/store.rs`, backed by the `keyring` crate. The keyring
service name is `com.omp-desktop.omp-desktop`; account names are
`<namespace>:<key>`. Two isolated namespaces (master design §8.1):

| Namespace | Holds |
|-----------|-------|
| `provider` | Agent Provider credentials (official / relay API keys) |
| `remote` | Remote IM channel credentials (bot tokens, app secrets) — one keychain account per instance field |

## What lands on disk

After migration, the app data directory contains **only opaque references**
of the form `keychain:v1:<namespace>:<key>` — never secret material:

| File | Contents |
|------|----------|
| `secrets.json` | `keychain:v1:provider:official_api_key` / `keychain:v1:provider:relay_api_key` references + non-secret settings |
| `remote/channel-secret-refs.json` | per-instance references such as `keychain:v1:remote:<instance>:bot_token` |
| `remote/channel-secrets.json` | legacy plaintext — read only as a pre-migration fallback and **deleted** once its last field migrates |

Reads of legacy unprefixed keychain accounts are supported for the `provider`
namespace only, so pre-migration installs keep working until migrated.

## Strict mode: no plaintext fallback

If the OS secure store is unavailable (no Secret Service provider on Linux,
or the keychain is locked):

- **Saving or refreshing credentials is blocked.** Nothing is written to
  disk, and the UI surfaces the actionable error
  `credentials.storeUnavailable` (all three locales): on Linux, start and
  unlock a Secret Service provider (e.g. gnome-keyring), then restart
  OMP Desktop. Existing credentials are left untouched.
- **Loads fail closed.** Once the migration ledger's `store_unavailable`
  flag is set, credentials are treated as absent rather than read from any
  plaintext source.

## Migration (§8.2)

The startup migration is idempotent and records every step in a
non-sensitive migration ledger (migration IDs and status only):

1. **Dry run** — enumerate legacy secrets, validate parseability, target
   namespace, and conflicts. Writes nothing.
2. **Copy** the secret into the OS secure store.
3. **Readback** with a constant-time comparison to verify equivalence.
4. **Reference commit** — write the `keychain:v1:` reference into
   `secrets.json` / `channel-secret-refs.json`.
5. **Tombstone** the legacy value (`__tombstoned_v1__`). All new writes go
   only to the OS store.
6. **Cleanup** — re-verify the reference resolves, then remove the legacy
   secret. `channel-secrets.json` is deleted when its last field migrates.

Failure semantics:

- Uncommitted references roll back; original values stay readable.
- Verified, committed items are never reverse-copied to plaintext.
- Cleanup failure keeps the tombstone and retries — migration never loses
  credentials.
- A store outage persists the ledger's `store_unavailable` flag (Safe Mode)
  and defers migration until the store is back; the app keeps running with
  credential loads failing closed.
- A second run with no legacy sources is a no-op.

Sources: provider keys from `secrets.json` (`SecretsJsonSource`) and remote
channel entries from `channel-secrets.json` (`ChannelSecretsSource`), both
in `src-tauri/src/secrets/migration.rs`.

## Operating guide

- **Add / rotate / clear a Provider key:** Settings → Providers → API key
  field. Writes go straight to the OS store; clearing removes the keychain
  entry and its reference.
- **Remote IM channel credentials:** entered in the Remote IM channel
  setup; stored under the `remote` namespace as one keychain account per
  instance field.
- **Linux "secure storage unavailable":** install, start, and unlock
  gnome-keyring (or another Secret Service provider), then restart the
  app — the deferred migration resumes automatically.
- **Inspect:** it is safe to grep `secrets.json` and
  `channel-secret-refs.json` — they should contain only `keychain:v1:`
  references. Never edit references by hand.

## Security properties and boundaries

Guaranteed today, with automated tests:

- Secrets live only in the OS secure store; keychain is the only backend
  (SA-C.1).
- Store outage blocks saves with an actionable error; no silent plaintext
  fallback (SA-C.3).
- Six-step migration idempotency, including constant-time readback,
  rollback, and tombstone-no-loss cleanup (SA-C.5 — a 16-test suite covers
  every §18.2.6 scenario).
- Remote channel credentials are isolated under the `remote` namespace and
  the legacy plaintext file is deleted after migration (SA-R.1).
- Logs record metadata only (counts, key names) — never secret material
  (SA-L.1).

Roadmap — **not yet implemented, do not rely on these** (master design §8.1
target architecture, tracked in `docs/release/security-audit-checklist.md`):

- auth-broker as the sole OAuth refresh writer shared by Desktop + CLI
  (SA-C.8).
- `CredentialIndexStore` metadata-only index (SA-C.9).
- `agent.db` storing only `keychain:v1:` references (SA-C.2).
- Unified keychain service/account naming shared with the CLI (SA-C.6).
- Release-artifact plaintext-secret scan (SA-C.7).
- Remote-namespace ACL and metadata-type isolation beyond namespacing
  (SA-C.4).

## File index

| File | Role |
|------|------|
| `src-tauri/src/secrets/store.rs` | `SecretStore` trait, `KeychainStore` (keyring), `MockStore` fault injection; namespaces and account naming |
| `src-tauri/src/secrets/mod.rs` | Provider credential load/save; strict-mode `keychain:v1:` resolution; `secrets.json` writer |
| `src-tauri/src/secrets/migration.rs` | Six-step `Migrator`, `MigrationLedger`, tombstone, channel adapter |
| `src-tauri/src/remote_im/config.rs` | Remote instance config; `channel-secret-refs.json` references; save blocked on store outage |
| `src-tauri/src/lib.rs` | `run_startup_migration` wiring |
| `src/i18n/messages.ts`, `src/i18n/zh-tw.ts` | `credentials.storeUnavailable` / `credentials.storeError` in en / zh-CN / zh-TW |
| `src/components/ProvidersPanel.tsx` | Provider key UI entry (Settings → Providers) |
````

- [ ] **Step 2: Brand gate**

Run:
```bash
cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand >/dev/null 2>&1 && echo BRAND_OK
```
Expected: `BRAND_OK`

- [ ] **Step 3: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main && git add docs/credential-management.md && git commit -m "docs(credentials): add credential management guide (AC-12.3)"
```

---

### Task 2: Matrix flip + README links + CHANGELOG

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md` (AC-12.3 row ~:250; counts table ~:343-348; FAIL list item 5 ~:380-382)
- Modify: `README.md` (docs table ~:174, after the 安全披露 row)
- Modify: `README_EN.md` (docs table ~:181, after the Security row)
- Modify: `CHANGELOG.md` (`### Added / 新增` under `## [0.3.1-nightly]`, after the update-channels bullet)

**Interfaces:**
- Consumes: `docs/credential-management.md` from Task 1.
- Produces: AC-12.3 PASS row; counts 41/16/100/1 (grep口径); bilingual README links; CHANGELOG entry.

- [ ] **Step 1: Flip the AC-12.3 row** — replace the entire old row:

```markdown
| AC-12.3 | Credential management guide (keychain, migration, no plaintext fallback) | Doc review | PASS | `docs/credential-management.md` (2026-07-31) documents the OS-store backends + service/account naming, `keychain:v1:` on-disk references, strict mode (no plaintext fallback, actionable `storeUnavailable` error), the §8.2 six-step startup migration + Safe Mode, operating recipes, and an honest-boundaries section listing unimplemented §8.1 components as roadmap. Behavior backed by the 16-test migration suite + secrets/store tests (506 cargo lib tests green). |
```

- [ ] **Step 2: Strike FAIL list item 5** — replace:

```markdown
5. **AC-12.3** — credential-management doc can now be written against the real
   §8.1/§8.2 behavior (strict mode + 6-step migration shipped 2026-07-31);
   the doc itself is not yet authored.
```

with:

```markdown
5. ~~**AC-12.3** — credential-management doc can now be written against the real
   §8.1/§8.2 behavior (strict mode + 6-step migration shipped 2026-07-31);
   the doc itself is not yet authored.~~
   **Resolved 2026-07-31**: `docs/credential-management.md` authored per
   `docs/superpowers/specs/2026-07-31-credential-management-doc-design.md` —
   keychain backends, `keychain:v1:` references, strict mode, six-step
   migration, operating guide, honest roadmap boundaries.
```

- [ ] **Step 3: Update the counts table** — `| PASS | 40 |` → `| PASS | 41 |` and `| FAIL | 2 |` → `| FAIL | 1 |`.

- [ ] **Step 4: Verify counts by grep**

Run:
```bash
cd /Users/po1nt9/Github/grok-app-main && grep -c "| PASS |" docs/release/1.0-acceptance-matrix.md; grep -o "\*\*PASS\*\*\|| PASS |" docs/release/1.0-acceptance-matrix.md | sort | uniq -c
```
Simpler authoritative check — count verdict cells in the matrix body:
```bash
cd /Users/po1nt9/Github/grok-app-main && for v in PASS PARTIAL BLOCKED FAIL; do printf "%s " "$v"; grep -oE "\| $v \|" docs/release/1.0-acceptance-matrix.md | wc -l; done
```
Expected: `PASS 41` / `PARTIAL 16` / `BLOCKED 100` / `FAIL 1` (the remaining FAIL cell is the counts-table row itself; zero real FAIL items). If a number disagrees, find the stray cell with `grep -n "| FAIL |" docs/release/1.0-acceptance-matrix.md` and reconcile before continuing.

- [ ] **Step 5: README.md** — add one row to the 文档 table, immediately after the `| 安全披露 | ... |` row:

```markdown
| 凭据管理 | [`docs/credential-management.md`](./docs/credential-management.md) |
```

- [ ] **Step 6: README_EN.md** — add one row to the Documentation table, immediately after the `| Security | ... |` row:

```markdown
| Credential management | [`docs/credential-management.md`](./docs/credential-management.md) |
```

- [ ] **Step 7: CHANGELOG.md** — append this bullet at the end of the `### Added / 新增` list under `## [0.3.1-nightly]` (after the update-channels bullet):

```markdown
- **Credential management guide:** new `docs/credential-management.md`
  documents where credentials live (OS secure store only — strict mode, no
  plaintext fallback), the `keychain:v1:` on-disk reference format, the
  six-step startup migration, and operator recipes (AC-12.3).
  新增凭据管理指南：凭据仅存系统安全存储（严格模式，无明文 fallback），
  记录 keychain:v1: 引用格式与六步启动迁移（AC-12.3）。
```

- [ ] **Step 8: Gates for Task 2**

Run:
```bash
cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand >/dev/null 2>&1 && echo BRAND_OK && test -f docs/credential-management.md && echo LINK_TARGET_OK
```
Expected: `BRAND_OK` + `LINK_TARGET_OK`

- [ ] **Step 9: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main && git add docs/release/1.0-acceptance-matrix.md README.md README_EN.md CHANGELOG.md && git commit -m "docs(release): AC-12.3 PASS — credential management guide (keychain, migration, no plaintext fallback)"
```

---

### Task 3: Full gates + memory + close-out

**Files:**
- Memory (outside repo): `/Users/po1nt9/.zcode/cli/memories/projects/github-858e378dd021e1c0/memory/omp-desktop-roadmap-status.md` + `MEMORY.md`

**Interfaces:**
- Consumes: Tasks 1-2 commits.
- Produces: green full-gate evidence; memory reflecting zero real FAIL items.

- [ ] **Step 1: Full gates**

Run:
```bash
cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -3 && pnpm test 2>&1 | tail -3 && pnpm typecheck && pnpm check:i18n 2>&1 | tail -1 && pnpm check:brand >/dev/null 2>&1 && echo BRAND_OK && pnpm check:provenance >/dev/null 2>&1 && echo PROVENANCE_OK && pnpm check:legal >/dev/null 2>&1 && echo LEGAL_OK
```
Expected: cargo `506 passed; 0 failed; 1 ignored` (if the documented `store` flake appears, re-run once); vitest `843 passed`; typecheck clean; i18n `OK (3 locales, 1889 keys each)`; all three echo lines. This package touches no code — any non-flake failure means an earlier task broke something; stop and investigate.

- [ ] **Step 2: Confirm working tree clean + commit log**

Run:
```bash
cd /Users/po1nt9/Github/grok-app-main && git status --short && git log --oneline -4
```
Expected: clean tree; the Task 1 + Task 2 commits on top of `0f81879` (spec) and `989c445` (AC-10.9).

- [ ] **Step 3: Memory update** — edit `omp-desktop-roadmap-status.md`:
  - frontmatter `description`: replace "剩余 1 个真实 FAIL（AC-12.3 凭据文档）" with "AC-12.3 凭据文档落地后真实 FAIL 归零，剩余 100 BLOCKED（真机/审计）".
  - Add bullet after the AC-10.9 entry: "**AC-12.3 凭据管理文档 ✅ 完成（2026-07-31，FAIL→PASS，spec `docs/superpowers/specs/2026-07-31-credential-management-doc-design.md` + plan 同名 plans/ 文件）**：`docs/credential-management.md`（英文，8 节：OS 后端+service/account 命名、`keychain:v1:` 磁盘引用、严格模式无明文 fallback、§8.2 六步迁移+Safe Mode、运维手册、诚实边界——SA-C.2/4/6/7/8/9 标为 roadmap、模块索引）。README 双语各加一行链接 + CHANGELOG 条目。无代码/测试变更（doc-review 验证）。矩阵 AC-12.3 → PASS，计数 **41 PASS / 16 PARTIAL / 100 BLOCKED / 1 FAIL（grep 口径，仅剩计数表自身一行；真实 FAIL = 0）**。"
  - How-to-apply paragraph: change "**1.0 仍不可发布**（1 个真实 FAIL + 100 BLOCKED）" → "**1.0 仍不可发布**（真实 FAIL 已归零；100 BLOCKED 待真机/外部审计）"; priorities → "①跨平台真机验收（§10 安装升级/OS×locale/性能基准/真实平台 smoke）→ ②外部安全审计 → ③AC-12.2/12.4/12.5/12.6/12.7 剩余文档包 + AC-12.8 OS-codesign caveat（均为 BLOCKED/PARTIAL 文档类，无需真机，可穿插）"。
  - `MEMORY.md` index line: update to "…+ AC-10.9 三渠道 + AC-12.3 凭据文档已落地；真实 FAIL 归零，剩余 100 BLOCKED，下一优先 跨平台真机验收 + 外部安全审计".

## Self-Review

1. **Spec coverage:** D1 → Task 1 (doc path/language/8-section structure all present in the embedded content); D2 → doc §"Security properties and boundaries" roadmap subsection; D3 → Task 2 Steps 5-6; D4 → Task 2 Steps 1-4; D5 → Task 3 Step 1 gates, no test tasks exist; D6 → Task 1. Matrix/README/CHANGELOG updates §4 of spec → Task 2. Acceptance §6 of spec → Task 2 Step 4 + Task 3 Step 1.
2. **Placeholder scan:** none — doc content embedded verbatim; every edit shows old and new text; all commands exact.
3. **Type/consistency:** doc filename `docs/credential-management.md` identical across Task 1 content, Task 2 matrix row/README rows/CHANGELOG bullet, and Task 2 Step 8 `test -f`. Counts 41/16/100/1 consistent between Step 3 edits and Step 4 expectation. Plan deviation noted vs spec §7: Task 2 Step 4's first grep command replaced by the authoritative verdict-cell count (the simpler one).
