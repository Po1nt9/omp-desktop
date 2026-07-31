# §12 Docs Batch (AC-12.2/12.4/12.5/12.6/12.7/12.8) — Design

**Date:** 2026-07-31 · **Status:** approved (user-absent; recommended defaults adopted and recorded here)
**Authority:** [Master Design §5.4/§10/§11.1/§12/§14.1/§18.2.7/§18.2.12](./2026-07-28-omp-desktop-design.md) · [1.0 acceptance matrix §12](../../release/1.0-acceptance-matrix.md) · [credential-management doc spec](./2026-07-31-credential-management-doc-design.md) (precedent)

## 1. Problem

Acceptance matrix §12 has five BLOCKED doc items whose unblocker is "author the guide", plus one PARTIAL with a residual caveat gap:

| AC | Item | Current | Gap |
|----|------|---------|-----|
| AC-12.2 | Provider setup guide | BLOCKED | no provider guide |
| AC-12.4 | Permission model guide | BLOCKED | `permission.rs` implemented (35 tests), undocumented |
| AC-12.5 | Remote access risk guide | BLOCKED | no remote-risk guide |
| AC-12.6 | Recovery boundary guide | BLOCKED | AC-1.10/AC-7.1 landed, undocumented |
| AC-12.7 | i18n guide | BLOCKED | 3 locales implemented (1889 keys), undocumented |
| AC-12.8 | Update guide | PARTIAL | OS-codesign caveat not fully reflected in `desktop-auto-update.md` |

All behaviors exist and are test-backed (grounded via code exploration 2026-07-31); only the docs are missing. Same class as AC-12.3 (closed 2026-07-31): verification = doc review.

## 2. Decisions (user-absent defaults)

| ID | Decision | Rationale |
|----|----------|-----------|
| D1 | Five standalone English guides in `docs/`: `provider-setup.md`, `permission-model.md`, `remote-access-risk.md`, `recovery-boundary.md`, `i18n-guide.md`; AC-12.8 closes via a small edit to `docs/desktop-auto-update.md` | One file per AC row = clean doc-review evidence; matches `credential-management.md` precedent. Alternatives rejected: single omnibus file (muddies per-AC evidence); README fold-in (bilingual burden ×5). |
| D2 | Uniform skeleton per guide: status blockquote → body sections → **Honest boundaries** → file index | Consistency with `docs/credential-management.md`. |
| D3 | **Honest negatives are mandatory content** (verified by code exploration): (a) Desktop has no OAuth flow — xAI OAuth is Runtime-side; the Desktop login handler is an inert stub (`App.tsx:8261`); (b) doctor `cli` probe currently reports `runtime_unavailable` (fail-closed stub, `commands.rs:1177`); (c) no global `allow_once` policy chip — once/session/always arrive per-request from the Runtime; (d) only LINE + WeCom(webhook mode) verify inbound signatures; the other 12 channels rely on platform-authenticated WS/long-poll transports; (e) Weixin personal ilink has no per-message signature; (f) no cross-restart `epoch+sequence` dedup — roadmap, needs stable event ids/replay cursors; (g) i18n interpolation is `{var}` only — no ICU plural/select; the check gate validates key parity/type/emptiness, NOT ICU types (design §12's ICU claim is roadmap); (h) no OS-locale auto-detect — default `zh`. | The AC-12.3 FAIL note's rule — docs must not claim guarantees the code doesn't meet — applies to every guide. |
| D4 | Matrix flips: AC-12.2/12.4/12.5/12.6/12.7 BLOCKED→PASS, AC-12.8 PARTIAL→PASS; counts PASS 41→47, PARTIAL 16→15, BLOCKED 100→95, FAIL 1 (self-row) unchanged. Also refresh AC-12.7 row's stale "1884 keys" → 1889. | Same flip procedure as AC-10.9/AC-12.3. |
| D5 | README.md + README_EN.md docs tables gain five rows each, placed immediately after the 凭据管理 / Credential management row | Bilingual sync rule; discoverability. |
| D6 | One CHANGELOG bullet under `## [0.3.1-nightly]` `### Added / 新增` covering the batch | Bilingual, matching existing entries. |
| D7 | No code/test/i18n-key changes; commit per task; gates = check:brand per task + full suite at the end | Doc-only package. |
| D8 | Guide language: English | docs/ convention. |

## 3. Per-guide content outline (grounded in code exploration)

### 3.1 `docs/provider-setup.md` (AC-12.2)
- Status: Runtime is **user-supplied** — Desktop does not bundle it.
- Point at the Runtime CLI: Settings → Runtime → CLI path card; manual path (trim-only validation) + native picker (`pick_cli_binary`); persisted as `manual_cli_path`; `cliInfo` probe (found/version/source/auth/checksum); "Allow unverified CLI install" toggle defaults off (fail-closed).
- Configure Providers: official row (xAI API key → OS keychain, link `credential-management.md`); custom providers (name/id/baseUrl/model/apiKey, protocol `responses`/`chat_completions`/`messages`); model Fetch button + datalist (catalog comes from Runtime; empty until supplied); activate; CC Switch import (scan → import, never auto-activates).
- Honest boundaries: no OAuth in Desktop (Runtime-side; inert stub); doctor `cli` probe reports `runtime_unavailable` in this build (auth/workspace/backend/logs checks live).
- File index: `SettingsPage.tsx` (CLI card), `ProvidersPanel.tsx`, `DoctorModal.tsx`, `commands.rs` (`pick_cli_binary`, `settings_set`, `doctor_report`), `store.rs` (`manual_cli_path`, `allow_unverified_cli_install`).

### 3.2 `docs/permission-model.md` (AC-12.4)
- Status: fail-closed, default `Ask`.
- Mode ladder table: `ask` (default) / `accept_edits` (edits auto, deletes never) / `allow_for_session` (session cache, in-project only) / `dont_ask` / `always_approve` (dangerous, two-step in-app confirm); internal ranks `AllowOnce` (=Ask rank), `Deny` (0).
- Per-path decisions: outside-project → only `always_approve` auto-allows; project root unresolvable → fail closed; session cache covers in-project paths only; `accept_edits` auto-allows edit tools **except deletes** (independent delete gate, §17.3); downloads into the project auto-allow; untrusted projects force `Ask`.
- Per-request approvals: Runtime (ACP) supplies once/session/always options per pending request; requests bind runtime/session/turn/request-id and die on timeout/restart/turn-end; first legal decision wins per pending request.
- Subagent clamp: effective policy is never wider than the parent (`subagent_effective_policy`).
- YOLO: two-step confirm dialog, per-session policy, remote yolo is TTL-bound (link remote guide).
- Persistent rules: Permission Rules panel edits `[permission] allow/deny/ask` in the Runtime home `config.toml`.
- Honest boundaries: no global `allow_once` chip; UI never fakes per-session persistence the Runtime didn't grant.
- File index: `permission.rs`, `PermissionRulesPanel.tsx`, `App.tsx` (policy dropdown + yolo confirm), `modelOptions.ts` (`PERMISSION_POLICIES`).

### 3.3 `docs/remote-access-risk.md` (AC-12.5)
- Status: 14 adapters implemented; official 1.0 support tier = 10 fixed + Weixin conditional (§14.1).
- Transport table: 12 platform-authenticated WS/long-poll channels vs 2 local webhook servers (LINE; WeCom in `connect_mode=webhook`) — both bind `127.0.0.1` by default; `allow_external` opt-in requires a reverse proxy/tunnel in front.
- Inbound defenses: signature verification (LINE `X-Line-Signature` HMAC-SHA256; WeCom `msg_signature` SHA1 when `callback_token` configured); ReplayGuard (±300 s freshness + per-channel nonce cache, webhook channels); DedupStore (SQLite, `(channel,message_id)`, 7-day TTL); RateLimiter (60/min per channel + 10/min per scope).
- Authorization: `allowFrom` sender whitelist (`*`/missing = open, `""` = disabled fail-closed, comma list = explicit senders); `require_mention` default true.
- Remote yolo: IM-granted approval TTL 3600 s, memory-only, never persisted, dies on restart; `allow_remote_yolo` master gate.
- Credentials: OS keychain `remote` namespace (link `credential-management.md`).
- Recommendations: set explicit `allowFrom` sender IDs before exposing a bridge; keep webhook bind on loopback with TLS-terminating proxy in front; rotate bot tokens; least-privilege bot scopes; MFA on the platform accounts (platform-side); prefer official-tier channels.
- Honest boundaries: only LINE/WeCom verify signatures — the rest trust platform-authenticated transport; Weixin personal has no per-message signature; signature-absent channels inherit platform compromise risk.
- File index: `remote_im/channels/{line,wecom}.rs` (webhook+sig), `replay_guard.rs`, `dedup_store.rs`, `rate_limiter.rs`, `outbound.rs` (allowFrom), `engine.rs` (approval TTL).

### 3.4 `docs/recovery-boundary.md` (AC-12.6)
- Status: conservative recovery — Desktop **never auto-replays**.
- Source of truth: the OMP session; Desktop journal stores UI projections/drafts/bindings/audit index only; UI history is never re-injected as a prompt (§10).
- Crash-mid-turn flow: TurnStart write-ahead → next launch `assess` → `Interrupted` → journal honestly closed (`TurnEnd{interrupted}`) + idempotent `turn_interrupted` marker message → UI chip (`endOfTurn.interrupted`, error tone) → user explicitly starts a new turn.
- Corrupt journal: quarantined to `event_journal.corrupt-<ts>.json`, fresh journal starts, no marker without evidence.
- Guarantees vs non-guarantees (§18.2.7 wording): guarantee = Desktop does not actively replay prompt/tool/edit/shell; non-guarantee = Runtime/tool side effects across the crash boundary are unknown — **no absolute no-duplication promise**; unknown boundaries are marked `unknown/interrupted`.
- Roadmap: cross-restart `epoch+sequence` dedup needs stable event ids + replay cursors from the Runtime (not claimable today).
- File index: `event_journal/recovery.rs`, `event_journal` (write-ahead), `session_manager` wiring, `src/lib/endOfTurn.ts`, i18n `endOfTurn.interrupted`.

### 3.5 `docs/i18n-guide.md` (AC-12.7)
- Status: three locales `en` / `zh-CN` / `zh-TW`, 1889 keys each, English = source + final fallback.
- For users: Settings → General → language picker; persists (`AppSettings.locale`); switches at runtime without restart; tray uses the Rust minimal locale; default `zh-CN`.
- For developers: catalogs `src/i18n/messages.ts` (en + zh) + `src/i18n/zh-tw.ts` (`zhTW`); adding a key = all three locales; `t(locale, key, vars)` / `createT`; locale aliases normalize (`zh-cn`→`zh`, `zh-tw`→`zh-TW`); fallback chain requested → en → key string.
- Runtime-visible content (§12 envelope): stable `messageKey` + typed `args`; without a stable key → "localized shell summary + viewable redacted raw"; never embed uncontrolled raw text into localized sentences.
- Coverage categories + exemptions: model output, user input, project files, raw tool output exempt; product/model names, commands, paths, code identifiers not translated; redacted raw Provider errors viewable as technical detail.
- Gate: `pnpm check:i18n` validates three-locale key parity (missing/extra), value types, and emptiness.
- Honest boundaries: `{var}` interpolation only — no ICU plural/select today; ICU parameter/type validation is design-roadmap, not in the gate; no OS-locale auto-detect (default `zh`).
- File index: `messages.ts`, `zh-tw.ts`, `index.ts` (`createT`/`normalizeLocale`), `scripts/check-i18n-completeness.mjs`, `tray_i18n` (Rust).

### 3.6 `docs/desktop-auto-update.md` edit (AC-12.8)
- Expand the status-blockquote caveat into a short "OS trust warnings" subsection mirroring `docs/release/signing-requirements.md`'s three-row table (macOS Gatekeeper → right-click Open / `xattr -dr com.apple.quarantine`; Windows SmartScreen → More info → Run anyway; Linux none) plus the cost note (Apple Developer ID $99/yr; OV/EV Authenticode ranges; SignPath Foundation free tier) and the existing link to signing-requirements. No other changes to that doc.

## 4. Matrix / README / CHANGELOG updates

- Six row flips with evidence (doc path + coverage summary + honest-boundary note); AC-12.7 stale "1884 keys" corrected to 1889 in the new row text.
- Counts: PASS 47 / PARTIAL 15 / BLOCKED 95 / FAIL 1 — verify by verdict-cell grep after edits.
- README.md / README_EN.md: five rows each after the credential-management row, same order as §3.
- CHANGELOG: one bilingual bullet referencing all five guides + the AC-12.8 caveat close.

## 5. Non-goals

- No code, test, or i18n-key changes (D7). The honest negatives are documentation content, not bugs to fix in this package.
- No Chinese translations of the guides (docs/ are English; READMEs stay bilingual).
- No new "BLOCKED items" summary surgery beyond the six row flips + counts (no such summary list exists for §12 doc rows beyond the rows themselves).
- Real-platform verification (remote channel smoke, performance) stays BLOCKED — untouched.

## 6. Acceptance

- Five guides exist with the §3 coverage and honest-boundaries sections; `desktop-auto-update.md` mirrors the signing-requirements caveat table.
- Matrix: six rows PASS; counts 47/15/95/1 verified by grep; zero stale "1884" reference.
- Both READMEs link all five guides; CHANGELOG entry added.
- All gates green: cargo 506+1, vitest 843, typecheck, check:i18n, check:brand, check:provenance, check:legal.

## 7. Self-review

- Placeholder scan: none — every guide's sections enumerate concrete, code-anchored facts (file:line in exploration report).
- Consistency: D3 honest negatives (a)-(h) each appear in exactly one guide's boundaries section; counts arithmetic (41+6=47, 16−1=15, 100−5=95) checks out; AC-12.8 flips PARTIAL not BLOCKED so PARTIAL drops by 1 and BLOCKED by 5.
- Scope: single plan, 8 tasks (5 guides + auto-update edit + matrix/README/CHANGELOG + gates/memory).
- Ambiguity: "official support tier = 10 fixed + Weixin conditional" follows §14.1, not the 14-adapter implementation set — the remote guide states both numbers explicitly to avoid conflation.
