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
