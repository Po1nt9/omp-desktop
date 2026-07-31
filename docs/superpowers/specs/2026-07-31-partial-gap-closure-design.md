# PARTIAL Gap-Closure Batch — Design

Date: 2026-07-31
Status: approved (user-absent precedent: recommended defaults adopted, decisions recorded here)
Scope: OMP Desktop 1.0 acceptance matrix — close the four PARTIAL rows whose gaps are addressable in-session (no real Runtime, no hardware, no external auditor).

## 1. Context

Matrix state after the §12 docs batch: **47 PASS / 15 PARTIAL / 95 BLOCKED / 1 FAIL** (grep cell counts; the single FAIL cell is the counts table self-count — real FAIL = 0).

The 15 PARTIAL rows were triaged:

- **11 rows** need the real Runtime (`omp` CLI), real IM channels, or real hardware — not addressable in-session.
- **4 rows** have gaps that are pure test/gate/evidence deficits, closable in-session:

| Row | Current gap (verbatim from matrix) |
| --- | --- |
| AC-3.4 | the script does not validate ICU placeholder (`{name}`) consistency across locales; `interpolate()` uses a simple regex, not ICU MessageFormat |
| AC-8.3 | no dedicated whitelist accept/reject unit test (only hardcoded `allowFrom: "*"` in engine test) |
| AC-8.7 | no key-rotation / in-flight credential revocation test (AC-8.17) |
| AC-8.14 | No actual image-fetch test for feishu/telegram/discord. Unblocker: manual media send + capture. |

Goal of this batch: AC-3.4 / AC-8.3 / AC-8.7 flip PARTIAL → PASS with code+test evidence; AC-8.14 gains real automated evidence but **stays PARTIAL** (its verification leg includes a manual media send, which needs real channels).

Expected matrix outcome: **50 PASS / 12 PARTIAL / 95 BLOCKED / 1 FAIL** (self-count).

## 2. Approaches considered

- **A. Targeted gap-closure batch (recommended, adopted).** Close exactly the four addressable gaps with the smallest honest change: one gate enhancement + three test additions + matrix/doc evidence updates. No production seams added for testability alone.
- **B. Defer everything to hardware arrival.** Rejected: these four gaps need no hardware; deferring leaves known-closeable debt.
- **C. Waive the PARTIALs.** Rejected: waiving is the user's call, not the assistant's.

## 3. Design decisions

### D1. AC-3.4 — placeholder-set consistency in the i18n gate

The named gap says the gate "does not validate ICU placeholder (`{name}`) consistency". Reality (already documented honestly in docs/i18n-guide.md): the app does **not** use ICU MessageFormat — `interpolate()` supports `{var}` substitution only. Therefore the complete, correct validation surface is: **for every key, the set of `{var}` placeholders must be identical across all 3 locales**. That is precisely what "placeholder consistency across locales" means for this codebase; there is no ICU syntax to validate.

Change to `scripts/check-i18n-completeness.mjs`:

- New exported helper `extractPlaceholders(text)` — returns the sorted unique set of `{name}` tokens via the same shape of regex the frontend `interpolate()` uses (`/\{(\w+)\}/g`).
- `checkCatalog(messages)` gains a placeholder-parity rule: for each en key whose value contains placeholders, each other locale's value must contain exactly the same set. Mismatch → error line listing key, locale, missing/extra placeholders, pushed into the same `errors` array the existing parity/type/emptiness checks use (so the gate exits non-zero exactly as today).
- Placeholder-free keys are unaffected. Extra placeholders in a translation are an error too (they would render literally as `{name}`).

Unit tests: **extend the existing `scripts/check-i18n-completeness.test.mjs`** (vitest, wired into `pnpm test`; imports `checkCatalog` and asserts error strings — same style as the five existing cases). New cases: placeholder parity OK passes; missing placeholder in a locale → error naming key + locale + missing/extra sets; extra placeholder in a translation → error; placeholder-free keys unaffected. Error string format follows the existing conventions (e.g. `zh.a.ok: placeholder mismatch (missing: name; extra: count)`).

Risk handling: the new rule runs against the real 1889-key catalogs. If genuine placeholder mismatches exist, the gate will fail — those are real i18n bugs and are fixed in the catalog files as part of this task (each fix verified against the en source string). If no mismatches exist, `pnpm check:i18n` stays green.

Doc sync: docs/i18n-guide.md §3 (gate description) and §6 (honest boundaries) updated — placeholder-set consistency is now enforced; the "no ICU MessageFormat (plural/select)" honest negative stays.

Matrix: AC-3.4 flips to PASS with a scope note: "ICU MessageFormat not in use (documented); gate now enforces `{var}` placeholder-set parity across all 3 locales — the complete validation surface for this codebase."

### D2. AC-8.3 — whitelist accept/reject unit tests

The enforcement code is `src-tauri/src/remote_im/outbound.rs` (`allow_from_list`, `sender_allowed`, `allow_from_blocks_enable`, `require_mention`) — pure functions, currently untested; the engine rejection path is engine.rs:383-391.

New tests in the existing `#[cfg(test)] mod tests` in `outbound.rs` (no new files, no production changes):

- `allow_from_list`: missing key → `None` (open); `"*"` → `None`; `""` → `Some(vec![])` (fail-closed); `"a,b"` → `Some(["a","b"])`; `" a , b "` trims whitespace; `allow_from` snake_case alias accepted.
- `sender_allowed`: open acl (`{}` / `"*"`) allows any sender; `""` rejects everyone (fail-closed); explicit list accepts listed sender, rejects unlisted sender.
- `allow_from_blocks_enable`: `""` blocks enable → true; `"*"`/missing → false.
- `require_mention`: default true (empty options/acl); `options.require_mention: false` → false; `acl.requireMention: false` → false.

Engine-level accept/reject: rejected senders take the reply-and-return path (engine.rs:383) whose reply goes through `OutboundRouter.reply` — with no live channel adapter the reply is swallowed (`let _ =`), so there is no observable state distinguishing reject from accept in the ephemeral engine. Decision: **pure-function coverage only**; the matrix evidence cites both the new unit tests and the enforcement point (engine.rs:383). This directly closes the named gap ("no dedicated whitelist accept/reject unit test") without inventing a mock channel adapter (YAGNI).

Matrix: AC-8.3 flips to PASS.

### D3. AC-8.7 — credential rotation / revocation tests

Two layers, both testable in-session:

1. **Config layer (rotation)** — new test in `src-tauri/src/remote_im/config.rs` test mod using the established template (`APP_HOME_ENV_LOCK` + `guard()` + `test_home` + `MockStore` + `install_test_store`): `save_instance` with `bot_token = "tok-v1"`, then `save_instance` again with `bot_token = "tok-v2"`. Assert: mock store returns `"tok-v2"` for `inst1:bot_token`; `get_secrets("inst1")` returns `"tok-v2"` immediately (no stale value); the refs file still holds exactly one reference row for the field (reference string stable across rotation); no plaintext of either value on disk.
2. **Router layer (revocation)** — new test in `outbound.rs`: `register("inst", ...)` then `unregister("inst")` → `secrets_for_test("inst")` returns `None`. (Register-then-secrets_for_test already covered at outbound.rs:289; the unregister leg is the revocation gap.)

Honest boundary (matrix evidence + docs/remote-access-risk.md wording already compatible): **running channel adapters capture secrets at register time; a rotation takes effect for a running instance on its next (re)start / re-register.** The tests assert the config + router layers reflect rotation/revocation immediately; they do not claim in-flight adapters hot-swap credentials. This matches §8.17's intent (rotation supported, revocation effective) without overstating.

Matrix: AC-8.7 flips to PASS with the boundary noted in the evidence cell.

### D4. AC-8.14 — Discord mock-fetch test; row stays PARTIAL

`fetch_attachment` (`src-tauri/src/remote_im/media.rs`) has three legs. Mockability:

- **Discord**: URL comes from the attachment itself → fully testable against a local HTTP server. Adopted.
- **Telegram**: base URL hardcoded to `https://api.telegram.org` — not testable without adding a production seam. Rejected (YAGNI; no test-only production change).
- **Feishu**: delegates to `channels::feishu::download_message_resource` (tenant token + API base) — same seam problem. Rejected.

New test in media.rs test mod: bind `std::net::TcpListener` on `127.0.0.1:0`, spawn a thread that accepts one connection and answers a minimal HTTP/1.1 response; `fetch_attachment("discord", &secrets, &options, &att)` with `AttachmentSource::Discord { url }` pointed at the listener. Two cases (two one-shot servers):

1. Response carries `Content-Type: image/jpeg` → returned bytes match the body, `mime_type == "image/jpeg"` (header honored).
2. Response omits Content-Type, URL ends in `.webp` → `mime_type == "image/webp"` (extension fallback).

No new dependencies (std TcpListener + thread + manual HTTP bytes; reqwest handles the client side).

Matrix: AC-8.14 **stays PARTIAL** — the row's verification includes a manual media send + capture on real channels, which needs hardware/accounts. Evidence cell updated: "Automated: Discord fetch leg covered by mock-HTTP test (media.rs); Telegram/Feishu legs require production seams (deferred, YAGNI). Remaining: manual media send + capture."

### D5. Matrix + docs + changelog updates

- Six cells touched across four rows (verdict + evidence each): AC-3.4, AC-8.3, AC-8.7 flip; AC-8.14 evidence only.
- Counts row: PASS 47→50, PARTIAL 15→12, BLOCKED 95, FAIL 1 (self-count) — recomputed by the grep loop, not by hand.
- docs/i18n-guide.md §3/§6 updated per D1.
- CHANGELOG.md: one bilingual bullet under `## [0.3.1-nightly]` → `### Added / 新增`: "Acceptance gap closure (AC-3.4 i18n placeholder parity gate; AC-8.3 whitelist tests; AC-8.7 rotation/revocation tests; AC-8.14 Discord fetch test)".
- README files: no change (no new docs pages).

### D6. Honest negatives preserved / added

- No ICU MessageFormat — placeholder-set parity is the full surface (stated in guide + matrix).
- No engine-level whitelist test (no observable without a mock adapter) — pure-function coverage + enforcement-point citation.
- Rotated credentials apply to running adapters on next (re)start — no hot-swap claim.
- Telegram/Feishu fetch legs untested (hardcoded base URLs; seam would be a test-only production change).
- AC-8.14 stays PARTIAL — manual leg outstanding.

### D7. Out of scope

- No production code changes for testability (no URL seams in media.rs, no mock channel adapter).
- The other 11 PARTIAL rows and all 95 BLOCKED rows (need real Runtime / hardware / auditor).
- No changes to frontend code, i18n catalog content (unless the new gate uncovers real mismatches — then the minimal catalog fix is in scope, per D1).

## 4. Verification

Per task, before commit:

- AC-3.4: `pnpm test -- scripts/check-i18n-completeness` (vitest file runs inside the existing suite); `pnpm check:i18n` (exit 0, 1889 keys × 3 locales — or, if real mismatches were found and fixed, the corrected count line).
- AC-8.3 / AC-8.7 / AC-8.14: `cd src-tauri && cargo test --lib` — 0 failures (watch for the known transient sandbox storm: immediate re-run converges).
- Final: full gate sweep — `cargo test --lib`, `pnpm vitest run`, `pnpm typecheck`, `pnpm check:i18n`, `pnpm check:brand`, `pnpm check:provenance`, `pnpm check:legal`.
- Matrix counts recomputed via the grep loop; expected 50/12/95/1.

## 5. Commit plan (English messages, AC tags)

1. `test(i18n): enforce {var} placeholder parity in completeness gate (AC-3.4)` — gate + vitest cases + guide update (+ catalog fixes if uncovered).
2. `test(remote_im): whitelist allow_from accept/reject unit tests (AC-8.3)`.
3. `test(remote_im): credential rotation + revocation tests (AC-8.7)`.
4. `test(remote_im): discord attachment fetch against mock HTTP server (AC-8.14)`.
5. `docs(release): flip AC-3.4/8.3/8.7 to PASS, refresh AC-8.14 evidence (50/12/95/1)` — matrix + CHANGELOG + memory.
