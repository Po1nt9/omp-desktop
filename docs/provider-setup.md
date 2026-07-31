# Provider setup

How OMP Desktop connects to the OMP Runtime CLI and how to configure model
providers (xAI and custom OpenAI-compatible endpoints).

> **Current status (2026-07-31):** the Runtime is **user-supplied** — Desktop
> does not bundle it. You point Desktop at an `omp` CLI binary you installed
> yourself, then configure providers in Settings. API keys go straight to the
> OS secure store (see [Credential management](./credential-management.md));
> Desktop never returns stored secrets to the UI.

## 1. Point Desktop at the Runtime CLI

Settings → Runtime → CLI path card:

- **Manual path** — paste the absolute path to the `omp` binary. Validation is
  trim-only; the probe below is the real check.
- **Browse…** — native file picker (`pick_cli_binary`), fills the same field.
- The path persists as `manual_cli_path` in the app settings store, applied
  via `api.settingsSet`.
- **CLI info probe** — reports found / version / source / auth status /
  checksum for the configured binary.
- **Allow unverified CLI install** — default **off** (fail-closed). Leave it
  off unless you deliberately run a self-built Runtime; enabling it lets an
  unverified binary act as the agent runtime.

## 2. Add your xAI API key (official provider)

Settings → Providers → the official xAI row: paste the API key into the key
box. The key is written via `api.secretsSet` into the OS keychain under the
`provider` namespace — it never lands in config files and is never read back
into the UI. Details: [Credential management](./credential-management.md).

## 3. Custom providers (OpenAI-compatible)

Settings → Providers → add a custom provider:

| Field | Meaning |
|---|---|
| Name / ID | display name and stable identifier |
| Base URL | the endpoint root |
| Model | model name — use **Fetch** to query the endpoint's model list and pick from the datalist |
| API key | stored in the OS keychain like the official key |
| Protocol | `responses`, `chat_completions`, or `messages` — pick what the endpoint speaks |

Then **Activate** the provider (`providersActivate`) to make it the runtime
default. The model catalog comes from the Runtime/endpoint — it is empty
until you supply a reachable endpoint + key.

## 4. Import from CC Switch

Settings → Providers → CC Switch import scans existing CC Switch provider
configs and imports them as custom providers. Import never auto-activates —
review and activate explicitly.

## 5. Diagnostics

Settings → Doctor runs health checks: auth, workspace, backend, logs.

**Honest caveat:** the `cli` connectivity probe currently reports
`runtime_unavailable` in this build (fail-closed stub in `doctor_report`) —
treat the other checks as live signal, and verify the CLI itself with the
CLI info probe in §1.

## 6. Honest boundaries

- **No OAuth in Desktop.** xAI OAuth (`omp` login) happens Runtime-side, in
  the CLI. The Desktop account-login handler is an inert stub today; do not
  expect a browser OAuth flow from the Desktop UI.
- Desktop never displays a stored key after save — the key box is write-only
  (design §5.4).

## 7. File index

| Area | File |
|---|---|
| CLI path card | `src/components/SettingsPage.tsx` |
| Providers UI | `src/components/ProvidersPanel.tsx` |
| Doctor modal | `src/components/DoctorModal.tsx` |
| Commands | `src-tauri/src/commands.rs` (`pick_cli_binary`, `settings_set`, `doctor_report`) |
| Settings persistence | `src-tauri/src/store.rs` (`manual_cli_path`, `allow_unverified_cli_install`) |
| Key storage | `docs/credential-management.md` |
