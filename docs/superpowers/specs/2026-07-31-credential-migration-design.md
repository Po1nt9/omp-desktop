# OMP Desktop 6-Step Credential Migration — Design Spec

**Date:** 2026-07-31
**Status:** Approved (strict mode + startup auto-migration confirmed by user; Plan A architecture recommended and accepted)
**Authority:** [Master Design §8.1/§8.2](./2026-07-28-omp-desktop-design.md) · [§18.2 pass condition 6](./2026-07-28-omp-desktop-design.md) · [Plan 10 Audit Summary](../../release/1.0-acceptance-matrix.md#audit-summary) · [security-audit-checklist SA-C](../../release/security-audit-checklist.md)
**Plan 10 role:** Highest-leverage remaining verified FAIL (AC-6.1/6.2/6.4/6.5/6.6/6.7, AC-2.10) after v1 transport injection landed.

---

## 1. Problem

Two plaintext credential stores exist on disk today, and the mandated 6-step
idempotent migration (master design §8.2) does not exist:

1. **`secrets.json`** (`src-tauri/src/secrets.rs`) — holds `official_api_key` /
   `relay_api_key`. The OS keychain is an **opt-in** (`store_api_keys_in_keychain`
   defaults to `false`), so the default mode is plaintext file storage. The
   existing `migrate_plaintext_keys_to_keychain` is a 2-step copy+clear: no
   dry-run, no readback verification, no reference indirection, no tombstone, no
   rollback, no ledger.
2. **`channel-secrets.json`** (`src-tauri/src/remote_im/config.rs`) — a plaintext
   `HashMap<instance_id, HashMap<field, value>>` for all Remote IM channel
   credentials (bot tokens, app secrets, webhook secrets). No keychain path at
   all.

Master design requirements being violated:

- **§8.1**: secrets must live in the OS secure store (macOS Keychain / Windows
  Credential Manager / Linux Secret Service) behind one unified Rust helper;
  remote-platform credentials use the same helper with an **isolated namespace**.
  When the secure store is unavailable, save/refresh must be **blocked with an
  actionable error — no silent plaintext fallback**.
- **§8.2**: legacy secrets migrate via 6 idempotent steps — dry-run → copy →
  readback (constant-time compare) → reference commit (`keychain:v1:<opaque-id>`)
  → tombstone → cleanup; every step records a non-sensitive migration ID + state;
  failures roll back uncommitted references, never lose credentials, never copy
  plaintext backwards.
- **§18.2 condition 6**: migration tests for dry-run / success / re-run /
  store-unavailable / readback-failure / cleanup-failure / rollback must all
  pass; release artifacts ship no plaintext secret fallback.

## 2. Goal

Implement the §8.2 6-step idempotent migration for both plaintext stores behind
a unified, mockable `SecretStore` abstraction, and switch the app to **strict
mode**: the OS secure store is the only credential backend; when unavailable,
credential save/load fails with an actionable error instead of falling back to
plaintext.

## 3. Non-Goals (YAGNI)

- OMP Runtime / CLI-side `agent.db` credential migration — the Runtime owns its
  own store; Desktop cannot and must not manage it.
- Full Safe Mode UI. This work only sets the migration-failure flag that the
  (separate) Safe Mode work package will consume; per master design, failed
  credential migration is a Safe Mode trigger.
- auth-broker OAuth-refresh single-writer rework (§8.1) — separate work package.
- Real-Runtime E2E of the v1 `credentials.*` methods — tracked in the acceptance
  matrix under its existing unblocker.
- Settings UI for migration status beyond a minimal status surface; the UI polish
  belongs to the settings work package.

## 4. Decisions (confirmed with user 2026-07-31)

| Decision | Choice | Rationale |
|---|---|---|
| Plaintext fallback policy | **Strict mode** | §8.1/§18.2.6 leave no room for a silent fallback; an explicit plaintext opt-in would still likely fail the "no plaintext secret fallback in release artifacts" bar. Linux users without Secret Service get an actionable error (install/unlock gnome-keyring or KWallet), which is the design's intent. |
| Migration trigger | **Automatic at startup** | §8.2 demands idempotent re-runnability; startup auto-run plus a settings "re-run migration" action satisfies both. Failed migration sets the Safe Mode flag per master design §security-mode triggers. |
| Architecture | **Plan A: `SecretStore` trait + one generic `Migrator` engine** | Five of the seven §18.2.6 test scenarios are fault-injection (unavailable / readback-fail / cleanup-fail / rollback / re-run); only a trait mock covers them cleanly. Plan B (inline + cfg(test) shims) duplicates the engine across two call sites and cannot simulate intra-process readback mismatch. This mirrors the `V1Transport` pattern landed earlier today. |

## 5. Architecture

```
src-tauri/src/secrets/
├── mod.rs            Public API: load/save/presence helpers (unchanged signatures)
├── store.rs          SecretStore trait + KeychainStore (prod) + StoreError
└── migration.rs      Migrator (6-step engine) + MigrationLedger + adapters

src-tauri/src/remote_im/config.rs   Channel credentials routed through SecretStore
```

### 5.1 `SecretStore` trait

```rust
pub trait SecretStore: Send + Sync {
    fn get(&self, ns: &str, key: &str) -> Result<Option<String>, StoreError>;
    fn set(&self, ns: &str, key: &str, value: &str) -> Result<(), StoreError>;
    fn delete(&self, ns: &str, key: &str) -> Result<(), StoreError>;
}

pub enum StoreError {
    /// OS secure store missing/locked — carries an actionable i18n message key.
    Unavailable { message_key: &'static str, detail: String },
    /// Backend reachable but the operation failed.
    Backend(String),
}
```

- `KeychainStore` wraps the existing `keyring` crate calls. Namespace maps into
  the account name as `<ns>:<key>` under the existing reverse-DNS service id —
  no change to service naming (§8.1 forbids Desktop inventing its own
  service/account scheme beyond the single shared helper).
- Namespaces: `"provider"` for `official_api_key` / `relay_api_key`; `"remote"`
  for channel credentials (`remote:<instance_id>:<field>`). Isolation is by
  construction — adapters never cross namespaces.
- `MockStore` (test-only, `#[cfg(test)]` — all migration tests are in-crate
  unit tests, so no feature flag is needed): in-memory map with programmable
  failure points (fail set / return wrong value on get / fail delete / fail the
  Nth call). This is what makes the seven §18.2.6 scenarios unit-testable.

### 5.2 `Migrator` — generic 6-step engine

One engine drives both stores. Each plaintext source provides a
`MigrationSource` adapter:

```rust
pub trait MigrationSource {
    /// Step 1 input: enumerate legacy plaintext entries.
    fn enumerate(&self) -> Result<Vec<LegacyEntry>, String>;
    /// Read the legacy plaintext value for an entry (dual-read path).
    fn read_legacy(&self, entry: &LegacyEntry) -> Result<Option<String>, String>;
    /// Step 4: atomically persist the `keychain:v1:<ns>:<opaque-id>` reference.
    fn commit_reference(&self, entry: &LegacyEntry, reference: &str) -> Result<(), String>;
    /// Step 5: replace the legacy value with a tombstone marker.
    fn tombstone(&self, entry: &LegacyEntry) -> Result<(), String>;
    /// Step 6: remove the legacy remnant after reference re-validation.
    fn cleanup(&self, entry: &LegacyEntry) -> Result<(), String>;
}

pub struct LegacyEntry {
    pub migration_id: String,   // non-sensitive stable id, e.g. "provider:official_api_key"
    pub namespace: &'static str,
    pub key: String,            // opaque id within the namespace
}
```

Two adapters ship in this work package:

- `SecretsJsonSource` — enumerates the two plaintext fields of `secrets.json`;
  references are written back into `secrets.json` (metadata + flags stay);
  cleanup strips the field; the file itself remains for non-secret metadata
  (`relay_base_url`, `default_model`).
- `ChannelSecretsSource` — enumerates every `(instance_id, field)` in
  `channel-secrets.json`; references are recorded in a new
  `channel-secret-refs.json` (non-secret); when every entry is `cleaned`, the
  legacy file is securely deleted (overwrite-then-remove) per §8.2 step 6.

### 5.3 The six steps (per entry, idempotent)

1. **dry_run** — enumerate; validate parseability of each legacy payload;
   check target namespace for conflicts (a live, non-tombstoned reference with
   a *different* value is a conflict and is reported, not overwritten). Writes
   nothing. Output: a `MigrationPlan` (entries + conflicts).
2. **copy** — `store.set(ns, key, legacy_value)`.
3. **readback** — `store.get(ns, key)`; constant-time compare with the legacy
   value. Mismatch → entry failed; roll back step 2 (`store.delete`); legacy
   value stays readable.
4. **reference** — adapter atomically commits `keychain:v1:<ns>:<opaque-id>`
   (temp-file + rename, same as existing atomic writers).
5. **tombstone** — legacy value replaced with the tombstone marker
   (`"__tombstoned_v1__"`); dual-read only applies to entries not yet
   tombstoned; **all new writes go to the store only**.
6. **cleanup** — re-resolve the reference (must read back the same value);
   then remove the legacy remnant. When the last channel entry cleans up,
   `channel-secrets.json` is securely deleted.

### 5.4 Failure semantics (verbatim from §8.2)

- Every step records the non-sensitive `migration_id` + new state in the
  **MigrationLedger** (`credential-migration.json`, mode 0600). States:
  `dry_run → copied → verified → referenced → tombstoned → cleaned`, plus
  `failed` with a non-sensitive reason code. The ledger makes every step
  re-runnable: a crash mid-pipeline resumes from each entry's recorded state.
- Steps 2–3 failure → delete the uncommitted store entry (rollback), legacy
  stays readable, ledger marks `failed`.
- Step 4 failure → store entry deleted (rollback), legacy stays readable.
- Steps 5–6 failure → **keep the tombstone, retry next run** — credentials are
  never lost.
- Verified/committed entries are **never** copied back to plaintext.
- Store unavailable at dry-run → whole run aborts before touching anything;
  the actionable error propagates and the migration-failure flag is set.

### 5.5 Strict mode (no silent plaintext fallback)

- `save_secrets` / channel `save_instance` write through `SecretStore` only.
  `StoreError::Unavailable` propagates as an actionable error (i18n key
  `credentials.storeUnavailable`; zh-CN copy includes the Linux Secret Service
  hint). **Nothing is written to disk in that path.**
- `load_secrets` / channel `get_secrets` resolve `keychain:v1:` references via
  the store; dual-read of a non-tombstoned legacy value exists **only** as the
  pre-migration read path. Post-migration code never originates plaintext.
- The `store_api_keys_in_keychain` setting is retired (treated as always-on);
  the settings toggle UI is removed in favor of migration status.
- At startup: if plaintext is detected and the store is unavailable, migration
  cannot run — set the migration-failure flag (Safe Mode trigger per master
  design) and block credential reads/writes with the actionable error rather
  than silently continuing on plaintext.

## 6. Data flow

**Startup:** `ensure_app_dirs` → load ledger → `Migrator::dry_run()` → if plan
is empty, done → else `Migrator::run(plan)` (steps 2–6 per entry, ledger
updated per step) → on abort/failure set the migration-failure flag.

**Save (strict):** caller → `save_secrets`/`save_instance` → `SecretStore::set`
→ reference/flags written to metadata file → plaintext nowhere.

**Load:** caller → metadata file → reference? `store.get` : (pre-migration only)
legacy plaintext.

## 7. Testing

All seven §18.2.6 scenarios as unit tests against `MockStore` + temp-dir files:

| # | Scenario | Assertions |
|---|---|---|
| 1 | dry-run | store untouched, files untouched, plan enumerates entries + conflicts |
| 2 | success | all entries `cleaned`; values equal; `channel-secrets.json` deleted; `secrets.json` holds only metadata + references |
| 3 | re-run | second run is a no-op (plan empty / entries already `cleaned`); no duplicate ledger entries beyond idempotent state confirmations |
| 4 | store unavailable | dry-run aborts; nothing written; actionable error surfaced; migration-failure flag set |
| 5 | readback failure | store entry rolled back (deleted); legacy value intact; ledger `failed`; re-run retries |
| 6 | cleanup failure | tombstone retained; value still resolvable via reference; next run completes cleanup |
| 7 | rollback (reference commit fails) | store entry deleted; legacy readable; ledger `failed` |

Plus: constant-time compare unit test (equal/unequal/length-mismatch);
namespace isolation test (same key under `provider` and `remote` does not
collide); corrupted-ledger recovery test (unknown state → entry re-enters
dry-run safely); channel adapter end-to-end (two instances × two fields).

Existing `secrets.rs` tests keep passing; tests asserting the old file-mode
default are updated to the strict-mode contract.

## 8. Acceptance-matrix impact

Directly addresses FAIL items **AC-6.1/6.2/6.4/6.5/6.6/6.7** (six-step
migration + tombstone + secure delete + no plaintext fallback) and **AC-2.10**
(migration test suite exists). SA-C.1 (plaintext channel creds) and SA-C.3
(silent plaintext fallback) — the two remaining HIGH security gaps from the
Phase 2 audit — are closed by §5.5 + §5.3 step 6.

## 9. Files

| File | Change |
|---|---|
| `src-tauri/src/secrets/store.rs` | **New** — SecretStore trait, KeychainStore, StoreError, MockStore (test) |
| `src-tauri/src/secrets/migration.rs` | **New** — Migrator, MigrationLedger, MigrationSource, both adapters |
| `src-tauri/src/secrets.rs` → `secrets/mod.rs` | Re-route load/save through SecretStore; strict mode; retire keychain opt-in |
| `src-tauri/src/remote_im/config.rs` | Channel secrets through SecretStore + reference file; secure delete of legacy |
| `src-tauri/src/store.rs` | Retire `store_api_keys_in_keychain`; keep setting field ignored for file-format compat |
| `src-tauri/src/lib.rs` | Startup migration run + failure flag wiring |
| `src/i18n/{en,zh-CN,zh-TW}` | `credentials.storeUnavailable` + migration status keys |
| `docs/release/1.0-acceptance-matrix.md` | Flip AC-6.x/AC-2.10 with test evidence |

## 10. Risks

- **Linux without Secret Service**: strict mode blocks credential save until a
  Secret Service provider is installed/unlocked — intended by design; the
  actionable error must name the remedy (gnome-keyring / KWallet).
- **Keychain unlock prompts on startup migration**: first migration may unlock
  the OS store once. Acceptable: one-time, user-visible, and the existing
  session cache prevents repeated prompts.
- **Dual-read window**: between tombstone and cleanup a crash leaves a
  tombstoned legacy file; the ledger + re-run semantics guarantee convergence,
  covered by scenario 6.
- **Settings file compat**: `store_api_keys_in_keychain` stays in the settings
  schema (ignored) so older configs still parse.
