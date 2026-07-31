# §12 Docs Batch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Author the five missing §12 user-facing guides and close the AC-12.8 codesign caveat, flipping six acceptance-matrix rows (AC-12.2/12.4/12.5/12.6/12.7 BLOCKED→PASS, AC-12.8 PARTIAL→PASS).

**Architecture:** Doc-only package. Five standalone English guides in `docs/` following the `credential-management.md` skeleton (status blockquote → body → honest boundaries → file index), one subsection edit to `docs/desktop-auto-update.md`, then matrix/README/CHANGELOG bookkeeping and full gates. No code, test, or i18n-key changes.

**Tech Stack:** Markdown docs, acceptance-matrix table edits, `pnpm check:brand` per task.

## Global Constraints

- Product name: **OMP Desktop**. Run `pnpm check:brand` before every commit.
- Guides are **English**; README.md is Chinese-primary, README_EN.md English — both get link rows (bilingual sync rule).
- **Honest negatives are mandatory** (spec D3): no Desktop OAuth (Runtime-side, inert stub); doctor `cli` probe reports `runtime_unavailable`; no global `allow_once` chip; only LINE + WeCom webhook verify inbound signatures; Weixin ilink has none; no cross-restart epoch+sequence dedup (roadmap); `{var}`-only interpolation, no ICU; gate checks parity/type/emptiness only; no OS-locale auto-detect (default `zh`).
- Matrix counts after flips: PASS 47 / PARTIAL 15 / BLOCKED 95 / FAIL 1 (self-row). Zero remaining `1884` references in the matrix (current real count: **1889 keys**, verified 2026-07-31 via `node scripts/check-i18n-completeness.mjs` → "check-i18n: OK (3 locales, 1889 keys each)").
- Commit per task, English commit messages with AC tags, on `main`.
- cwd resets between Bash calls — prefix every command with `cd /Users/po1nt9/Github/grok-app-main &&`.
- macOS: no `timeout` command; zsh glob-mangles bare `===` — quote separators (`echo "---"`).

---

### Task 1: `docs/provider-setup.md` (AC-12.2)

**Files:**
- Create: `docs/provider-setup.md`

**Interfaces:**
- Consumes: spec §3.1 facts (grounded via code exploration 2026-07-31).
- Produces: doc linked later from README tables and matrix AC-12.2 row.

- [ ] **Step 1: Write the guide**

Create `docs/provider-setup.md` with exactly this content:

````markdown
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
````

- [ ] **Step 2: Brand gate**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand`
Expected: PASS (no output violations).

- [ ] **Step 3: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main && git add docs/provider-setup.md && git commit -m "docs(providers): add provider setup guide (AC-12.2)"
```

---

### Task 2: `docs/permission-model.md` (AC-12.4)

**Files:**
- Create: `docs/permission-model.md`

**Interfaces:**
- Consumes: spec §3.2 facts.
- Produces: doc linked from README tables and matrix AC-12.4 row.

- [ ] **Step 1: Write the guide**

Create `docs/permission-model.md` with exactly this content:

````markdown
# Permission model

How OMP Desktop decides whether a tool call needs your approval — approval
modes, per-path decisions, fail-closed rules.

> **Current status (2026-07-31):** the permission ladder + path-aware
> auto-allow rules are implemented in `src-tauri/src/permission.rs` (35 tests)
> and wired to the session policy dropdown, the YOLO two-step confirm,
> per-request approval options from the Runtime, and the `[permission]` rules
> panel. Default mode is **Ask**: everything prompts unless a rule below says
> otherwise.

## 1. Approval modes

Settings / chat-header policy dropdown (`PERMISSION_POLICIES`):

| Mode | Auto-allows | Notes |
|---|---|---|
| `ask` (default) | nothing | every actionable tool call prompts |
| `accept_edits` | edit tools **except deletes** | the delete carve-out (§3) always still prompts |
| `allow_for_session` | approved tool+path, remembered for this session | session cache covers **in-project** paths only |
| `dont_ask` | suppresses prompts where policy permits | lower rank than `ask` — see ladder |
| `always_approve` | everything, including outside-project paths | **dangerous** — enabling requires the two-step in-app confirm |

Internal ranks:
`AlwaysApprove(5) > AllowForSession(4) > AcceptEdits(3) > Ask(2) = AllowOnce(2) > DontAsk(1) > Deny(0)`
(`permission.rs`). Higher wins when policies combine.

## 2. Per-path decisions

`may_auto_allow` decides per tool call, per path:

| Situation | Decision |
|---|---|
| Path **outside** the project root | only `always_approve` auto-allows; everything else prompts |
| Project root cannot be canonicalized | **fail closed** — prompt |
| Session-cached approval (from `allow_for_session`) | auto-allow only for paths inside the project |
| `accept_edits` + edit tool + in-project | auto-allow — unless the tool is a delete (§3) |
| Downloads into the project directory | auto-allow |
| Project marked untrusted | force `ask` regardless of mode |

## 3. The delete carve-out

Delete tools (`delete_file`, `rm`, `remove`, any tool id containing
"delete") **never** inherit edit approval — even under `accept_edits` they
prompt. Deletes are irreversible; they are their own risk class (§17.3).

## 4. Per-request approvals

Each pending approval dialog offers the options the Runtime (ACP) sent for
that request — typically **once / session / always** (`pick_option_id`).
Requests bind runtime/session/turn/request-id and die on timeout, restart, or
turn end; the first legal decision wins per pending request.

**Honest caveat:** there is no global "allow once" chip in the mode dropdown —
once/session/always are per-request choices supplied by the Runtime, not
modes you preset.

## 5. Subagents

A subagent's effective policy is the **narrower of** its own and its parent's
(`subagent_effective_policy`) — a child can never widen what the parent
disallowed.

## 6. YOLO mode

The YOLO toggle (`always_approve` for the session) requires a two-step
in-app confirm dialog (App `setAppDialog` — no `window.confirm`). Remote (IM)
yolo is separate: TTL-bound and memory-only — see
[Remote access risk](./remote-access-risk.md).

## 7. Persistent rules

Settings → Permission Rules panel edits `[permission]` `allow` / `deny` /
`ask` lists in the Runtime home `config.toml`. Explicit rules apply on top of
the mode ladder.

## 8. Honest boundaries

- No global `allow_once` chip (§4).
- The UI never fakes persistence the Runtime didn't grant — "session" scopes
  end with the session.
- Pending approvals do not survive a restart.

## 9. File index

| Area | File |
|---|---|
| Ladder + path rules | `src-tauri/src/permission.rs` |
| Policy dropdown options | `src/constants/modelOptions.ts` (`PERMISSION_POLICIES`) |
| Rules panel | `src/components/PermissionRulesPanel.tsx` |
| YOLO confirm + session policy wiring | `src/App.tsx` |
````

- [ ] **Step 2: Brand gate**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main && git add docs/permission-model.md && git commit -m "docs(permissions): add permission model guide (AC-12.4)"
```

---

### Task 3: `docs/remote-access-risk.md` (AC-12.5)

**Files:**
- Create: `docs/remote-access-risk.md`

**Interfaces:**
- Consumes: spec §3.3 facts.
- Produces: doc linked from README tables and matrix AC-12.5 row; cross-linked from `permission-model.md` §6 (Task 2 content already links it).

- [ ] **Step 1: Write the guide**

Create `docs/remote-access-risk.md` with exactly this content:

````markdown
# Remote access risk

Security posture of OMP Desktop's Remote IM bridge — which channels exist,
how inbound messages are verified, and the knobs you must set before exposing
a bridge.

> **Current status (2026-07-31):** 14 channel adapters are implemented under
> `src-tauri/src/remote_im/channels/`. The official 1.0 support tier is **10
> fixed channels + Weixin (personal) conditional** (design §14.1); the rest are
> best-effort. Remote approvals (yolo) are TTL-bound and never persisted.

## 1. Transport per channel

| Transport | Channels | Inbound authentication |
|---|---|---|
| Platform-authenticated WebSocket / long-poll | dingtalk, discord, feishu, matrix, qq, qqbot, slack (Socket Mode), telegram, weibo, weixin (ilink), wps_xiezuo, wecom (WS mode) | the platform session itself — no per-message signature |
| Local webhook server (binds a port) | **LINE**, **wecom** (`connect_mode=webhook`) | per-request signature (§2) |

Webhook servers bind `127.0.0.1` by default. `allow_external` is opt-in and
assumes you put a TLS-terminating reverse proxy/tunnel in front.

## 2. Signature verification

- **LINE** — `X-Line-Signature` HMAC-SHA256 over the raw body, verified
  against the channel secret.
- **WeCom webhook** — `msg_signature` SHA1 verification when `callback_token`
  is configured.

**Honest boundary:** only these two verify inbound signatures. The
WS/long-poll channels trust the platform-authenticated connection — if the
platform account or token is compromised, forged inbound is possible. Weixin
personal ilink has no per-message signature at all.

## 3. Replay, dedup, rate limits

- **ReplayGuard** — webhook channels reject messages outside a ±300 s
  freshness window and cache seen `channel|nonce` pairs. (WS/long-poll
  channels pass through — platform ordering applies.)
- **DedupStore** — SQLite `(channel, message_id)` `INSERT OR IGNORE`; 7-day
  TTL with a sweep every 1024 inserts.
- **RateLimiter** — 60 msg/min per channel + 10 msg/min per scope.

## 4. Who may talk to the agent: `allowFrom`

Per-channel sender whitelist (`outbound.rs`):

| Value | Behavior |
|---|---|
| unset or `*` | **open** — anyone who can reach the bot talks to the agent |
| `""` (empty string) | **fail-closed** — channel disabled |
| comma-separated sender IDs | only those senders |

`require_mention` (default **true**) additionally requires @-mentioning the
bot in group contexts. Unauthorized senders get a "not on allow_from list"
rejection.

**Before you expose a bridge, set explicit sender IDs.** The open default is
convenient for a first smoke test and unsafe beyond it.

## 5. Remote approvals (yolo)

- IM-granted yolo has a **3600 s TTL**, lives in memory only, is never written
  to disk, and dies on restart (`DEFAULT_APPROVAL_TTL_SECS`, engine + bridge
  wiring).
- The `allow_remote_yolo` master gate must be on for remote approval at all.
- Anti-replay for approvals rides on §3 (AC-8.4, shipped 2026-07-31).

## 6. Credentials

Bot tokens live in the OS secure store under the `remote` namespace,
referenced on disk as `keychain:v1:remote:<key>` — see
[Credential management](./credential-management.md). Rotate tokens on any
suspicion of leak.

## 7. Recommendations

1. Set explicit `allowFrom` sender IDs per channel (§4).
2. Keep webhook binds on loopback; terminate TLS at a proxy in front.
3. Prefer official-tier channels; treat the rest as best-effort.
4. Least-privilege bot scopes on each platform; enable MFA on the platform
   accounts themselves (that side is the platform's, not Desktop's).
5. Keep `allow_remote_yolo` off unless you actively need IM approvals;
   remember the 1 h TTL.

## 8. Honest boundaries

- Signature verification exists only for LINE + WeCom webhook (§2).
- Weixin personal ilink: no per-message signature; conditional support tier.
- Rate limits are in-memory — they reset on restart.
- The 14 implemented adapters ≠ the 10-channel official support tier.

## 9. File index

| Area | File |
|---|---|
| Channel adapters | `src-tauri/src/remote_im/channels/*.rs` |
| LINE / WeCom webhook + signatures | `src-tauri/src/remote_im/channels/line.rs`, `wecom.rs` |
| Replay guard | `src-tauri/src/remote_im/replay_guard.rs` |
| Dedup | `src-tauri/src/remote_im/dedup_store.rs` |
| Rate limiter | `src-tauri/src/remote_im/rate_limiter.rs` |
| allowFrom / mention gating | `src-tauri/src/remote_im/outbound.rs` |
| Approval TTL | `src-tauri/src/remote_im/engine.rs`, `bridge.rs` |
````

- [ ] **Step 2: Brand gate**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main && git add docs/remote-access-risk.md && git commit -m "docs(remote): add remote access risk guide (AC-12.5)"
```

---

### Task 4: `docs/recovery-boundary.md` (AC-12.6)

**Files:**
- Create: `docs/recovery-boundary.md`

**Interfaces:**
- Consumes: spec §3.4 facts.
- Produces: doc linked from README tables and matrix AC-12.6 row.

- [ ] **Step 1: Write the guide**

Create `docs/recovery-boundary.md` with exactly this content:

````markdown
# Recovery boundary

What OMP Desktop guarantees after a crash — and what it explicitly does not.

> **Current status (2026-07-31):** conservative recovery is implemented
> (AC-1.10 / AC-7.1). Desktop **never auto-replays** a prompt, tool call,
> edit, or shell command after a crash. Interrupted turns are honestly closed
> and marked; corrupt journals are quarantined, not repaired.

## 1. Source of truth

The **OMP session** (Runtime-side) is the source of truth. The Desktop event
journal stores UI projections — drafts, bindings, audit index — only. UI
history is **never re-injected as a prompt** (design §10).

## 2. Crash mid-turn

1. Every turn starts with a **write-ahead** `TurnStart`
   (`append_turn_start_durable`) — if the app dies mid-turn, the journal
   proves the turn was open.
2. Next launch, `assess` replays from the last commit and checks turn depth →
   `RecoveryState::Interrupted`.
3. Recovery closes the turn honestly: appends
   `TurnEnd{stopReason:"interrupted"}` (idempotent — a second assess returns
   `Clean`) plus a marker message (`turn_interrupted|crash_recovery`).
4. The UI renders an **interrupted** chip (`endOfTurn.interrupted`, error
   tone) at the end of the affected history.
5. You decide what to do next — Desktop does not resume or replay anything on
   its own.

## 3. Corrupt journal

An unparseable journal is renamed to `event_journal.corrupt-<timestamp>.json`
and a fresh journal starts; the state reports `Clean` and **no marker is
fabricated** — recovery acts only on evidence.

## 4. Guarantees vs non-guarantees

| Guaranteed | Not guaranteed |
|---|---|
| Desktop never actively replays a prompt/tool/edit/shell after a crash | Runtime/tool **side effects** across the crash boundary (a tool may have run partially) — unknown, marked `unknown/interrupted` |
| Interrupted turns are closed + marked, idempotently | **No absolute no-duplication promise** (§18.2.7) — an in-flight Runtime action may or may not have completed |
| Corrupt journals are preserved (quarantined) for inspection | Cross-restart `epoch+sequence` event dedup — **roadmap**, needs stable event ids + replay cursors from the Runtime |

## 5. What to do as a user

After the interrupted chip: start a new turn explicitly. If the turn had side
effects (file edits, shell), check the workspace before re-issuing the
instruction — the model will see the current file state, not a replay.

## 6. File index

| Area | File |
|---|---|
| Recovery assess/close | `src-tauri/src/event_journal/recovery.rs` |
| Write-ahead TurnStart | `src-tauri/src/event_journal/` |
| Session wiring | `src-tauri/src/session_manager.rs` |
| UI end-of-turn reason | `src/lib/endOfTurn.ts` + i18n key `endOfTurn.interrupted` |
````

- [ ] **Step 2: Brand gate**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main && git add docs/recovery-boundary.md && git commit -m "docs(recovery): add recovery boundary guide (AC-12.6)"
```

---

### Task 5: `docs/i18n-guide.md` (AC-12.7)

**Files:**
- Create: `docs/i18n-guide.md`

**Interfaces:**
- Consumes: spec §3.5 facts; key count **1889** (verified 2026-07-31).
- Produces: doc linked from README tables and matrix AC-12.7 row.

- [ ] **Step 1: Write the guide**

Create `docs/i18n-guide.md` with exactly this content:

````markdown
# Internationalization (i18n)

Supported locales, runtime switching, message catalogs, and the localization
envelope for Runtime-visible content.

> **Current status (2026-07-31):** three locales — **English (`en`),
> 简体中文 (`zh-CN`), 繁體中文 (`zh-TW`)** — 1889 message keys each, enforced
> by `pnpm check:i18n` (key parity + value types + non-empty). English is the
> source catalog and the final fallback.

## 1. Switching language

Settings → General → language picker. The choice persists
(`AppSettings.locale`) and applies **at runtime, without restart**. Default
locale is `zh-CN`. The system tray uses a minimal Rust-side locale of its
own.

**Honest boundary:** there is no OS-locale auto-detect — fresh installs start
in `zh-CN` until you switch.

## 2. Catalog layout (for developers)

| Locale | File |
|---|---|
| `en` + `zh` | `src/i18n/messages.ts` |
| `zh-TW` | `src/i18n/zh-tw.ts` (`zhTW` export) |

- Adding a key = adding it to **all three** locales in the same commit; the
  gate fails otherwise.
- `Locale = "zh" | "zh-TW" | "en"` (`messages.ts`); `normalizeLocale` maps
  aliases (`zh-cn`/`zh-hans` → `zh`, `zh-tw` → `zh-TW`, others → `en`).
- Lookup: `t(locale, key, vars)` / the `createT(locale)` helper; fallback
  chain requested locale → `en` → the key string itself.

## 3. Interpolation

Simple `{var}` substitution only:

```ts
t(locale, "sessions.deleteConfirm", { name })
```

**Honest boundary:** no ICU MessageFormat — no plural/select rules today.
Design §12's ICU parameter/type validation is roadmap; the current gate
checks key parity, value types, and emptiness, not ICU correctness. Write
copy that works without plurals (e.g. `"{count} item(s)"`).

## 4. Runtime-visible content (envelope)

Content crossing from the Runtime (tool results, errors, approvals) follows
the §12 envelope: a **stable `messageKey` + typed args**, rendered by the
shell. If no stable key exists, the shell shows a **localized summary + a
viewable redacted raw payload** — never embed uncontrolled raw text inside a
localized sentence.

## 5. What is not translated

- Model output, user input, project files, raw tool output (exempt
  categories).
- Product and model names, commands, file paths, code identifiers.
- Redacted raw Provider error payloads remain viewable as technical detail.

## 6. The gate

```sh
pnpm check:i18n
```

Validates, across all three locales: key parity (missing/extra), value types,
non-empty values. It is part of 1.0 acceptance (AC-2.5 / AC-3.1–3.3).

## 7. File index

| Area | File |
|---|---|
| en + zh catalogs | `src/i18n/messages.ts` |
| zh-TW catalog | `src/i18n/zh-tw.ts` |
| `createT` / `normalizeLocale` | `src/i18n/index.ts` |
| Completeness gate | `scripts/check-i18n-completeness.mjs` |
| Tray strings | `src-tauri/src/tray_i18n.rs` |
````

- [ ] **Step 2: Brand gate**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main && git add docs/i18n-guide.md && git commit -m "docs(i18n): add i18n guide (AC-12.7)"
```

---

### Task 6: `docs/desktop-auto-update.md` OS-codesign caveat (AC-12.8)

**Files:**
- Modify: `docs/desktop-auto-update.md` (status blockquote ends "...independent of OS trust." around line 16, followed by `## Update channels (AC-10.9)`)

**Interfaces:**
- Consumes: `docs/release/signing-requirements.md` three-row table + cost figures.
- Produces: evidence cited by matrix AC-12.8 row.

- [ ] **Step 1: Insert the OS trust warnings subsection**

In `docs/desktop-auto-update.md`, find this exact anchor (the end of the status blockquote + the next heading):

```markdown
> Plan 9 work. The minisign keypair verifies *archive integrity*, independent of
> OS trust.

## Update channels (AC-10.9)
```

Replace with:

```markdown
> Plan 9 work. The minisign keypair verifies *archive integrity*, independent of
> OS trust.

## OS trust warnings while unsigned

Until OS code-signing lands (Plan 9), expect — and verify past — the platform
warnings:

| Platform | Unsigned consequence | User workaround |
|----------|---------------------|-----------------|
| macOS | Gatekeeper blocks launch ("unidentified developer") | Right-click → Open, or `xattr -dr com.apple.quarantine /Applications/OMP\ Desktop.app` |
| Windows | SmartScreen "Windows protected your PC" | "More info" → "Run anyway" |
| Linux | None (AppImage is unsigned by design) | — |

Signing costs (details + secret inventory in
[Signing requirements](./release/signing-requirements.md)): Apple Developer
Program **USD $99/year**; Windows Authenticode OV roughly **$100–300/year**,
EV roughly **$300–700/year**; the SignPath Foundation offers a free tier for
open-source projects.

## Update channels (AC-10.9)
```

- [ ] **Step 2: Verify the edit**

Run: `cd /Users/po1nt9/Github/grok-app-main && grep -n "OS trust warnings" docs/desktop-auto-update.md`
Expected: one match (the new heading).

- [ ] **Step 3: Brand gate + commit**

```bash
cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand && git add docs/desktop-auto-update.md && git commit -m "docs(updater): mirror OS codesign warning table + signing costs (AC-12.8)"
```

---

### Task 7: Matrix flips + README/CHANGELOG bookkeeping

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md` (rows 249, 251–255; counts ~344-347; rows 57, 74–76 for the 1884→1889 refresh)
- Modify: `README.md` (after line 175, the 凭据管理 row)
- Modify: `README_EN.md` (after line 182, the Credential management row)
- Modify: `CHANGELOG.md` (after the credential-management bullet under `## [0.3.1-nightly]` → `### Added / 新增`)

**Interfaces:**
- Consumes: the five guides + auto-update edit from Tasks 1–6 (must exist on disk).
- Produces: final acceptance state — PASS 47 / PARTIAL 15 / BLOCKED 95 / FAIL 1.

- [ ] **Step 1: Flip AC-12.2 row**

In `docs/release/1.0-acceptance-matrix.md`, replace this exact row:

```markdown
| AC-12.2 | Provider setup guide | Doc review | BLOCKED | Runtime is user-supplied (Settings → Manual CLI path documented). Full Provider setup walkthrough (xAI OAuth, model selection) not in docs. **Unblocker:** author provider guide. |
```

with:

```markdown
| AC-12.2 | Provider setup guide | Doc review: docs/provider-setup.md | PASS | `docs/provider-setup.md` (2026-07-31) covers pointing Desktop at the user-supplied Runtime CLI (manual path + picker + CLI info probe, unverified-install toggle default off), xAI key entry via the OS keychain, custom providers (fields, protocol choice, model fetch), CC Switch import, and doctor — with honest boundaries: OAuth is Runtime-side (Desktop login is an inert stub) and the doctor `cli` probe reports `runtime_unavailable` in this build. |
```

- [ ] **Step 2: Flip AC-12.4 row**

Replace:

```markdown
| AC-12.4 | Permission model guide (per-path decision table, approval modes, fail-closed) | Doc review | BLOCKED | Permission model implemented + tested (permission.rs, 35 tests) but not documented in a user-facing guide. **Unblocker:** author permission guide. |
```

with:

```markdown
| AC-12.4 | Permission model guide (per-path decision table, approval modes, fail-closed) | Doc review: docs/permission-model.md | PASS | `docs/permission-model.md` (2026-07-31) documents the 5-mode ladder + internal ranks, the per-path decision table (outside-project → AlwaysApprove only, uncanonicalizable root → fail closed, session cache in-project only, untrusted → Ask), the delete carve-out, per-request once/session/always from the Runtime, subagent narrower-of-two clamp, YOLO two-step confirm, and `[permission]` TOML rules. Honest boundary: no global `allow_once` chip. Behavior backed by permission.rs (35 tests). |
```

- [ ] **Step 3: Flip AC-12.5 row**

Replace:

```markdown
| AC-12.5 | Remote access risk guide (channel security, MFA recommendation, whitelist, least privilege) | Doc review | BLOCKED | No remote-risk guide. **Unblocker:** author guide (especially given §8 security gaps found). |
```

with:

```markdown
| AC-12.5 | Remote access risk guide (channel security, MFA recommendation, whitelist, least privilege) | Doc review: docs/remote-access-risk.md | PASS | `docs/remote-access-risk.md` (2026-07-31) documents the 14 implemented adapters vs the 10+1 official support tier, transport/signature table (only LINE + WeCom webhook verify inbound signatures; Weixin ilink has none), ReplayGuard ±300 s + nonce, DedupStore 7-day TTL, RateLimiter 60/min channel + 10/min scope, `allowFrom` whitelist semantics (open/`""` fail-closed/explicit IDs) + `require_mention`, 3600 s memory-only yolo TTL + `allow_remote_yolo` gate, keychain `remote` namespace, and recommendations (explicit sender IDs, loopback + TLS proxy, MFA on platform accounts, least-privilege bot scopes). |
```

- [ ] **Step 4: Flip AC-12.6 row**

Replace:

```markdown
| AC-12.6 | Recovery boundary guide (conservative recovery, unknown/interrupted, no absolute no-duplication promise) | Doc review | BLOCKED | No recovery-boundary guide. **Unblocker:** author guide after AC-1.10/AC-7 wiring. |
```

with:

```markdown
| AC-12.6 | Recovery boundary guide (conservative recovery, unknown/interrupted, no absolute no-duplication promise) | Doc review: docs/recovery-boundary.md | PASS | `docs/recovery-boundary.md` (2026-07-31) documents OMP-session-as-source-of-truth, write-ahead TurnStart + assess + idempotent interrupted close + `turn_interrupted` marker, corrupt-journal quarantine (no fabricated markers), and an explicit guarantees/non-guarantees table: Desktop never actively replays, but Runtime/tool side effects across the crash boundary are unknown (marked `unknown/interrupted`), no absolute no-duplication promise (§18.2.7), and cross-restart epoch+sequence dedup is roadmap. |
```

- [ ] **Step 5: Flip AC-12.7 row (also fixes the stale 1884 key count)**

Replace:

```markdown
| AC-12.7 | i18n guide (supported locales, runtime switching, envelope format) | Doc review | BLOCKED | i18n implemented (3 locales, 1884 keys) but no user-facing locale guide. **Unblocker:** author guide. |
```

with:

```markdown
| AC-12.7 | i18n guide (supported locales, runtime switching, envelope format) | Doc review: docs/i18n-guide.md | PASS | `docs/i18n-guide.md` (2026-07-31) documents the 3 locales (en/zh-CN/zh-TW, 1889 keys each), runtime switching with `zh-CN` default (honest boundary: no OS-locale auto-detect), catalog layout + `createT`/`normalizeLocale`, `{var}`-only interpolation (ICU plural/select is roadmap; the gate checks parity/types/emptiness only), the §12 envelope (stable messageKey + typed args, localized summary + redacted raw fallback), and exemption categories. |
```

- [ ] **Step 6: Flip AC-12.8 row**

Replace:

```markdown
| AC-12.8 | Update guide (channels, updater, signing) | Doc review: docs/desktop-auto-update.md | PARTIAL | docs/desktop-auto-update.md documents minisign updater + per-channel feeds (updated for AC-10.9, 2026-07-31). **Gap:** the OS-codesign caveat is not fully reflected. |
```

with:

```markdown
| AC-12.8 | Update guide (channels, updater, signing) | Doc review: docs/desktop-auto-update.md | PASS | docs/desktop-auto-update.md documents minisign updater + per-channel feeds (AC-10.9) and now mirrors the signing-requirements OS-warning table (macOS Gatekeeper → right-click Open / `xattr`; Windows SmartScreen → More info → Run anyway; Linux none) plus signing costs (Apple $99/yr, OV/EV ranges, SignPath free tier) — caveat gap closed 2026-07-31. |
```

- [ ] **Step 7: Refresh stale 1884 key counts (rows 57, 74, 75, 76)**

Replace (row 57, AC-2.5):

```markdown
| AC-2.5 | i18n completeness check | `node scripts/check-i18n-completeness.mjs` — exit 0 | PASS | exit 0; "3 locales, 1884 keys each" (audit run 2026-07-31). |
```

with:

```markdown
| AC-2.5 | i18n completeness check | `node scripts/check-i18n-completeness.mjs` — exit 0 | PASS | exit 0; "3 locales, 1889 keys each" (re-verified 2026-07-31). |
```

Replace **all three** occurrences (rows 74, 75, 76 — AC-3.1/3.2/3.3) of:

```markdown
1884 keys, 0 violations (audit run 2026-07-31).
```

with:

```markdown
1889 keys, 0 violations (re-verified 2026-07-31).
```

- [ ] **Step 8: Update verdict counts**

Replace:

```markdown
| PASS | 41 | Verified with evidence |
| PARTIAL | 16 | Core present but a gap noted (test, feature, or doc) |
| BLOCKED | 100 | Cannot verify without real devices / external auditor / real-Runtime E2E |
```

with:

```markdown
| PASS | 47 | Verified with evidence |
| PARTIAL | 15 | Core present but a gap noted (test, feature, or doc) |
| BLOCKED | 95 | Cannot verify without real devices / external auditor / real-Runtime E2E |
```

(FAIL row stays `| FAIL | 1 | …` — the counts-table self-row.)

- [ ] **Step 9: Verify matrix by grep**

Run:

```bash
cd /Users/po1nt9/Github/grok-app-main && for v in PASS PARTIAL BLOCKED FAIL; do printf "%s " "$v"; grep -oE "\| $v \|" docs/release/1.0-acceptance-matrix.md | wc -l; done && grep -c "1884" docs/release/1.0-acceptance-matrix.md; true
```

Expected: `PASS 47`, `PARTIAL 15`, `BLOCKED 95`, `FAIL 1`, and the `1884` count is `0`.

- [ ] **Step 10: README.md link rows**

In `README.md`, find:

```markdown
| 凭据管理 | [`docs/credential-management.md`](./docs/credential-management.md) |
```

Replace with:

```markdown
| 凭据管理 | [`docs/credential-management.md`](./docs/credential-management.md) |
| Provider 配置 | [`docs/provider-setup.md`](./docs/provider-setup.md) |
| 权限模型 | [`docs/permission-model.md`](./docs/permission-model.md) |
| 远程访问风险 | [`docs/remote-access-risk.md`](./docs/remote-access-risk.md) |
| 恢复边界 | [`docs/recovery-boundary.md`](./docs/recovery-boundary.md) |
| 国际化 (i18n) | [`docs/i18n-guide.md`](./docs/i18n-guide.md) |
```

- [ ] **Step 11: README_EN.md link rows**

In `README_EN.md`, find:

```markdown
| Credential management | [`docs/credential-management.md`](./docs/credential-management.md) |
```

Replace with:

```markdown
| Credential management | [`docs/credential-management.md`](./docs/credential-management.md) |
| Provider setup | [`docs/provider-setup.md`](./docs/provider-setup.md) |
| Permission model | [`docs/permission-model.md`](./docs/permission-model.md) |
| Remote access risk | [`docs/remote-access-risk.md`](./docs/remote-access-risk.md) |
| Recovery boundary | [`docs/recovery-boundary.md`](./docs/recovery-boundary.md) |
| Internationalization (i18n) | [`docs/i18n-guide.md`](./docs/i18n-guide.md) |
```

- [ ] **Step 12: CHANGELOG bullet**

In `CHANGELOG.md`, find the end of the credential-management bullet (the bilingual entry added for AC-12.3 under `## [0.3.1-nightly]` → `### Added / 新增`). Append immediately after it:

```markdown
- **Guides batch (§12):** five new user-facing guides — provider setup,
  permission model, remote access risk, recovery boundary, and i18n — plus an
  OS-codesign warning table with signing costs in the update guide.
  (AC-12.2/12.4/12.5/12.6/12.7/12.8.)
  指南批量补齐（§12）：新增 Provider 配置、权限模型、远程访问风险、恢复边界、
  i18n 五篇指南，并在更新指南中补充 OS 签名警告表与签名成本。
```

(Anchor on the last line of the AC-12.3 bullet — verify with `grep -n "credential" CHANGELOG.md | head -3` first; the bullet spans ~6 lines ending with its Chinese sentence.)

- [ ] **Step 13: Brand gate + commit**

```bash
cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand && git add docs/release/1.0-acceptance-matrix.md README.md README_EN.md CHANGELOG.md && git commit -m "docs(release): AC-12.2/12.4/12.5/12.6/12.7/12.8 PASS — §12 guides batch; counts 47/15/95/1"
```

---

### Task 8: Full gates + memory update

**Files:**
- Modify: `/Users/po1nt9/.zcode/cli/memories/projects/github-858e378dd021e1c0/memory/omp-desktop-roadmap-status.md`
- Modify: `/Users/po1nt9/.zcode/cli/memories/projects/github-858e378dd021e1c0/memory/MEMORY.md`

**Interfaces:**
- Consumes: all Tasks 1–7 committed.
- Produces: final green-gate evidence + updated project memory.

- [ ] **Step 1: Run full gates**

Run each from `/Users/po1nt9/Github/grok-app-main` (prefix every command with `cd /Users/po1nt9/Github/grok-app-main &&`):

1. `pnpm check:brand` → PASS
2. `node scripts/check-i18n-completeness.mjs` → `check-i18n: OK (3 locales, 1889 keys each)`
3. `pnpm typecheck` → exit 0
4. `pnpm test` (vitest) → 95 files / 843 tests passed (read the summary with `grep -E "Tests|Test Files"`, not `tail -3` — tail swallows the Tests line)
5. `cd src-tauri && cargo test --lib` → 506 passed + 1 ignored. **Known flakes:** `store::tests::ensure_general_project_is_idempotent_and_not_removable` is a documented intermittent sandbox failure — re-run. If a docs-only package ever shows a large failure storm (e.g. ~15 at once), re-run first before investigating — transient sandbox storms were observed 2026-07-31 and converge on re-run.
6. `pnpm check:provenance` → PASS
7. `pnpm check:legal` → PASS

- [ ] **Step 2: Update project memory**

**Read the memory file first** (Edit requires a Read in this conversation), then in
`/Users/po1nt9/.zcode/cli/memories/projects/github-858e378dd021e1c0/memory/omp-desktop-roadmap-status.md`:

1. Update the frontmatter `description:` line to end with: `…AC-12.3 凭据文档 + §12 文档批（12.2/12.4/12.5/12.6/12.7/12.8）均已落地；真实 FAIL 归零，剩余 95 BLOCKED（真机/审计）`
2. Append to the body (after the AC-12.3 bullet):

```markdown
- **§12 文档批（2026-07-31）**：五篇指南 + AC-12.8 caveat 一次落地（spec fe45b48 / plan 见 docs/superpowers/plans/2026-07-31-docs-batch.md）——provider-setup、permission-model、remote-access-risk、recovery-boundary、i18n-guide + desktop-auto-update.md 补 OS 签名警告表；矩阵 6 行翻转，计数 PASS 47 / PARTIAL 15 / BLOCKED 95 / FAIL 1（自查行）；1884→1889 键数全量刷新（含 AC-2.5/3.1/3.2/3.3 历史行）。文档强制写入诚实否定项（无 Desktop OAuth、doctor cli 探针 fail-closed、仅 LINE/WeCom 验签、无 ICU、无 OS locale 检测等）。
```

3. In the same file's How-to-apply priorities, change item ③ to: `③（已完成）§12 文档批` and keep ①跨平台真机验收 / ②外部安全审计 as the only remaining paths to 1.0.
4. Update `MEMORY.md`'s omp-desktop-roadmap-status hook line to: `Plan 1-9 完成；§12 文档批落地（47 PASS / 15 PARTIAL / 95 BLOCKED / 真实 FAIL=0），剩余真机验收+外部审计`

- [ ] **Step 3: Commit memory is N/A (memory dir is outside the repo) — final verification instead**

Run: `cd /Users/po1nt9/Github/grok-app-main && git status --short && git log --oneline -10`
Expected: clean working tree; commits for the five guides, the auto-update edit, the matrix flip, and (from earlier) the spec/plan.

---

## Self-Review

**1. Spec coverage:** D1 (five files + auto-update edit) → Tasks 1–6. D2 (uniform skeleton) → every embedded guide follows status→body→honest boundaries→file index. D3 honest negatives (a)-(h): (a)(b) → Task 1 §6/§5; (c) → Task 2 §4/§8; (d)(e) → Task 3 §2/§8; (f) → Task 4 §4; (g)(h) → Task 5 §3/§1 + §1. D4 (six flips, counts, 1884→1889) → Task 7 Steps 1–9 (all five stale `1884` rows covered, matching spec §6 "zero stale 1884 reference"). D5 → Task 7 Steps 10–11. D6 → Task 7 Step 12. D7 (no code changes, commit per task, gates) → all tasks + Task 8. D8 (English guides) → embedded content is English.

**2. Placeholder scan:** every write/edit step contains the full verbatim content; every command is exact with expected output. No TBD/TODO/"similar to".

**3. Type/consistency:** doc filenames are identical across Tasks 1–6 (creates), Task 7 (matrix/README links), and cross-links (`permission-model.md` §6 links `remote-access-risk.md`, created in Task 3 — order is safe: Task 3 commits before anything references it in committed content? Task 2's guide links a not-yet-existing file for one commit; accepted, same pattern as credential doc linking precedent; final state is consistent). Counts arithmetic: 41+6=47 PASS, 16−1=15 PARTIAL, 100−5=95 BLOCKED. Key count 1889 consistent across Task 5 guide, Task 7 matrix text, and the verified gate output.
