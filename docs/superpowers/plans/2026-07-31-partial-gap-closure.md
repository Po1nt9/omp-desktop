# PARTIAL Gap-Closure Batch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four in-session-addressable PARTIAL gaps in the 1.0 acceptance matrix (AC-3.4 placeholder parity gate, AC-8.3 whitelist tests, AC-8.7 rotation/revocation tests, AC-8.14 Discord mock-fetch test) and flip the matrix to 50 PASS / 12 PARTIAL / 95 BLOCKED / 1 FAIL (self-count).

**Architecture:** One gate enhancement (`scripts/check-i18n-completeness.mjs`) + three test-only additions inside existing `#[cfg(test)]` modules (`outbound.rs`, `config.rs`, `media.rs`) + matrix/CHANGELOG/guide evidence updates. No production code changes for testability (spec D7).

**Tech Stack:** Node ESM check scripts + vitest; Rust 2021 + tokio + reqwest; std `TcpListener` for the mock HTTP server (no new dependencies).

**Spec:** `docs/superpowers/specs/2026-07-31-partial-gap-closure-design.md` (commit f9a22a4)

## Global Constraints

- Repo root: `/Users/po1nt9/Github/grok-app-main`. `cd` there in every Bash call (cwd resets between calls).
- Commit per task, English message with AC tag, on `main` (no push).
- Log metadata only — never log secret values or prompt content.
- No production seams added for testability alone (no URL injection in `media.rs`, no mock channel adapter).
- Known flake: transient sandbox storms can fail ~15 cargo tests at once on test-only packages — immediate re-run converges. Do not "fix" storms.
- Matrix verdict counts are computed by grep, never by hand:
  ```sh
  for v in PASS PARTIAL BLOCKED FAIL; do printf "%s " "$v"; grep -oE "\| $v \|" docs/release/1.0-acceptance-matrix.md | wc -l; done
  ```
- The single FAIL cell is the counts-table self-count; real FAIL = 0. It must stay exactly 1.
- zsh: quote any `===` separators; macOS has no `timeout` command.
- vitest summary: `grep -E "Test Files|Tests "` (not `tail`).

---

### Task 1: AC-3.4 — placeholder-set parity in the i18n gate

**Files:**
- Modify: `scripts/check-i18n-completeness.mjs` (add `extractPlaceholders` export + parity rule in `checkCatalog`)
- Modify: `scripts/check-i18n-completeness.test.mjs` (extend existing vitest describe)
- Modify: `docs/i18n-guide.md` (§3 honest boundary, §6 gate description)

**Interfaces:**
- Consumes: existing `checkCatalog(messages) -> string[]` (violations list; empty = pass); existing error string conventions (`zh: missing key "b.title"`, `zh.a.ok: value is empty or whitespace-only`).
- Produces: `extractPlaceholders(text: string) -> string[]` (sorted unique `{var}` names, same regex shape as frontend `src/i18n/index.ts:20` `/\{(\w+)\}/g`); new violation format `${loc}.${key}: placeholder mismatch (missing: a, b; extra: c)` with either clause omitted when empty. Task 5 relies on the gate exit code + violation text only.

- [ ] **Step 1: Write the failing tests**

Replace the import line at the top of `scripts/check-i18n-completeness.test.mjs`:

```js
import { checkCatalog, extractPlaceholders } from "./check-i18n-completeness.mjs";
```

Append inside the existing `describe("check-i18n-completeness", ...)` block (after the "fails when en is empty" case):

```js
  it("extractPlaceholders returns sorted unique names", () => {
    expect(extractPlaceholders("{b} and {a} and {b}")).toEqual(["a", "b"]);
    expect(extractPlaceholders("no placeholders")).toEqual([]);
  });

  it("passes when placeholders match across locales", () => {
    const messages = {
      en: { "a.greet": "Hello {name}, you have {count} item(s)" },
      zh: { "a.greet": "你好 {name}，你有 {count} 个项目" },
    };
    expect(checkCatalog(messages)).toEqual([]);
  });

  it("flags a placeholder missing from a translation", () => {
    const messages = {
      en: { "a.greet": "Hello {name}, you have {count} item(s)" },
      zh: { "a.greet": "你好 {name}" },
    };
    const v = checkCatalog(messages);
    expect(v).toContain("zh.a.greet: placeholder mismatch (missing: count)");
  });

  it("flags an extra placeholder in a translation", () => {
    const messages = {
      en: { "a.greet": "Hello {name}" },
      zh: { "a.greet": "你好 {name}，共 {count} 项" },
    };
    const v = checkCatalog(messages);
    expect(v).toContain("zh.a.greet: placeholder mismatch (extra: count)");
  });

  it("flags missing and extra placeholders together", () => {
    const messages = {
      en: { "a.greet": "Hello {name}" },
      zh: { "a.greet": "你好 {user}" },
    };
    const v = checkCatalog(messages);
    expect(v).toContain("zh.a.greet: placeholder mismatch (missing: name; extra: user)");
  });

  it("ignores placeholder-free keys", () => {
    const messages = {
      en: { "a.ok": "OK" },
      zh: { "a.ok": "好的" },
    };
    expect(checkCatalog(messages)).toEqual([]);
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run scripts/check-i18n-completeness.test.mjs 2>&1 | grep -E "Test Files|Tests |FAIL|extractPlaceholders"`
Expected: FAIL — `extractPlaceholders is not a function` / no export.

- [ ] **Step 3: Implement the gate enhancement**

In `scripts/check-i18n-completeness.mjs`, add after the `transpileTs` function (before `loadMessagesModule`):

```js
const PLACEHOLDER_RE = /\{(\w+)\}/g;

/**
 * Sorted unique `{var}` placeholder names in a message string. Same regex
 * shape as the frontend `interpolate()` (src/i18n/index.ts).
 */
export function extractPlaceholders(text) {
  const set = new Set();
  for (const m of text.matchAll(PLACEHOLDER_RE)) set.add(m[1]);
  return [...set].sort();
}
```

In `checkCatalog`, after the `enKeys` guard (after the `if (enKeys.size === 0) {...}` block), precompute en placeholder sets:

```js
  const enTable = messages.en ?? {};
  const enPlaceholders = new Map();
  for (const k of enKeys) {
    const v = enTable[k];
    if (typeof v === "string") enPlaceholders.set(k, extractPlaceholders(v));
  }
```

Inside the per-locale missing-keys loop, extend the string-value branch. Current code:

```js
      const v = table[k];
      if (typeof v !== "string") {
        violations.push(`${loc}.${k}: value is not a string (got ${typeof v})`);
      } else if (v.trim().length === 0) {
        violations.push(`${loc}.${k}: value is empty or whitespace-only`);
      }
```

Replace with:

```js
      const v = table[k];
      if (typeof v !== "string") {
        violations.push(`${loc}.${k}: value is not a string (got ${typeof v})`);
      } else if (v.trim().length === 0) {
        violations.push(`${loc}.${k}: value is empty or whitespace-only`);
      } else if (loc !== "en" && enPlaceholders.has(k)) {
        const want = enPlaceholders.get(k);
        const got = extractPlaceholders(v);
        const missing = want.filter((p) => !got.includes(p));
        const extra = got.filter((p) => !want.includes(p));
        if (missing.length || extra.length) {
          const parts = [];
          if (missing.length) parts.push(`missing: ${missing.join(", ")}`);
          if (extra.length) parts.push(`extra: ${extra.join(", ")}`);
          violations.push(`${loc}.${k}: placeholder mismatch (${parts.join("; ")})`);
        }
      }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm vitest run scripts/check-i18n-completeness.test.mjs 2>&1 | grep -E "Test Files|Tests "`
Expected: `Test Files  1 passed`, `Tests  11 passed` (5 existing + 6 new).

- [ ] **Step 5: Run the gate against the real 1889-key catalogs**

Run: `pnpm check:i18n`
Expected: `check-i18n: OK (3 locales, 1889 keys each)`, exit 0.
If placeholder mismatches surface instead, they are real i18n bugs: fix the offending catalog value(s) in `src/i18n/messages.ts` / `src/i18n/zh-tw.ts` to match the en placeholder set (verify each fix against the en source string), then re-run until green. Record any such fixes in the commit message.

- [ ] **Step 6: Update docs/i18n-guide.md §3 + §6**

§3 — replace:

```md
**Honest boundary:** no ICU MessageFormat — no plural/select rules today.
Design §12's ICU parameter/type validation is roadmap; the current gate
checks key parity, value types, and emptiness, not ICU correctness. Write
copy that works without plurals (e.g. `"{count} item(s)"`).
```

with:

```md
**Honest boundary:** no ICU MessageFormat — no plural/select rules today.
Design §12's ICU parameter/type validation is roadmap; since 2026-07-31 the
gate enforces `{var}` placeholder-set parity across locales (AC-3.4) — the
complete validation surface for this codebase — but there is still no ICU
syntax to check. Write copy that works without plurals (e.g.
`"{count} item(s)"`).
```

§6 — replace:

```md
Validates, across all three locales: key parity (missing/extra), value types,
non-empty values. It is part of 1.0 acceptance (AC-2.5 / AC-3.1–3.3).
```

with:

```md
Validates, across all three locales: key parity (missing/extra), value types,
non-empty values, and `{var}` placeholder-set parity (AC-3.4). It is part of
1.0 acceptance (AC-2.5 / AC-3.1–3.4).
```

- [ ] **Step 7: Commit**

```bash
git add scripts/check-i18n-completeness.mjs scripts/check-i18n-completeness.test.mjs docs/i18n-guide.md
git commit -m "test(i18n): enforce {var} placeholder parity in completeness gate (AC-3.4)"
```

---

### Task 2: AC-8.3 — whitelist accept/reject unit tests

**Files:**
- Modify: `src-tauri/src/remote_im/outbound.rs` (test mod only; production code untouched)

**Interfaces:**
- Consumes: `allow_from_list(&Value) -> Option<Vec<String>>`, `sender_allowed(&Value, &str) -> bool`, `allow_from_blocks_enable(&Value) -> bool`, `require_mention(&Value, &Value) -> bool` (outbound.rs:232-272); `json!` already in scope in the test mod via `use super::*` (the module imports `serde_json::json` at file top — verify; if not, add `use serde_json::json;` inside the test mod).
- Produces: 4 new test fns. Task 3 adds a 5th test to the same mod.

- [ ] **Step 1: Write the tests (test-only change; they pass against existing pure functions — the gap is missing coverage, not missing behavior)**

Append inside `#[cfg(test)] mod tests` in `src-tauri/src/remote_im/outbound.rs`, after `register_always_injects_instance_id`:

```rust
    /// AC-8.3: allow_from parsing — open / fail-closed / explicit list /
    /// whitespace + comma tolerance / snake_case alias.
    #[test]
    fn allow_from_list_parses_open_fail_closed_and_lists() {
        assert_eq!(allow_from_list(&json!({})), None); // missing → open
        assert_eq!(allow_from_list(&json!({ "allowFrom": "*" })), None);
        assert_eq!(allow_from_list(&json!({ "allowFrom": " * " })), None);
        assert_eq!(allow_from_list(&json!({ "allowFrom": "" })), Some(vec![])); // fail-closed
        assert_eq!(
            allow_from_list(&json!({ "allowFrom": "ou_a,ou_b" })),
            Some(vec!["ou_a".to_string(), "ou_b".to_string()])
        );
        assert_eq!(
            allow_from_list(&json!({ "allowFrom": " ou_a , ou_b ," })),
            Some(vec!["ou_a".to_string(), "ou_b".to_string()])
        );
        assert_eq!(
            allow_from_list(&json!({ "allow_from": "ou_a" })),
            Some(vec!["ou_a".to_string()])
        );
    }

    /// AC-8.3: whitelist accept/reject decisions.
    #[test]
    fn sender_allowed_accepts_and_rejects_per_allow_from() {
        // Open ACL allows anyone.
        assert!(sender_allowed(&json!({}), "ou_x"));
        assert!(sender_allowed(&json!({ "allowFrom": "*" }), "ou_x"));
        // Fail-closed empty list rejects everyone.
        assert!(!sender_allowed(&json!({ "allowFrom": "" }), "ou_x"));
        // Explicit list: accept listed, reject unlisted.
        let acl = json!({ "allowFrom": "ou_a, ou_b" });
        assert!(sender_allowed(&acl, "ou_a"));
        assert!(sender_allowed(&acl, "ou_b"));
        assert!(!sender_allowed(&acl, "ou_evil"));
        // Sender ids are matched exactly (no trimming of the sender side).
        assert!(!sender_allowed(&acl, " ou_a"));
    }

    /// AC-8.3: enable is blocked only by an explicit empty allow list.
    #[test]
    fn allow_from_blocks_enable_only_on_empty_list() {
        assert!(allow_from_blocks_enable(&json!({ "allowFrom": "" })));
        assert!(!allow_from_blocks_enable(&json!({})));
        assert!(!allow_from_blocks_enable(&json!({ "allowFrom": "*" })));
        assert!(!allow_from_blocks_enable(&json!({ "allowFrom": "ou_a" })));
    }

    /// AC-8.3: require_mention defaults to true; options/acl can opt out.
    #[test]
    fn require_mention_defaults_true_with_overrides() {
        assert!(require_mention(&json!({}), &json!({})));
        assert!(!require_mention(&json!({ "require_mention": false }), &json!({})));
        assert!(!require_mention(&json!({}), &json!({ "requireMention": false })));
        assert!(require_mention(
            &json!({ "require_mention": true }),
            &json!({ "requireMention": false })
        ));
    }
```

- [ ] **Step 2: Run the remote_im outbound tests**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib remote_im::outbound 2>&1 | tail -5`
Expected: 5 passed (1 existing + 4 new), 0 failed. On a sandbox storm: re-run immediately.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/remote_im/outbound.rs
git commit -m "test(remote_im): whitelist allow_from accept/reject unit tests (AC-8.3)"
```

---

### Task 3: AC-8.7 — credential rotation + revocation tests

**Files:**
- Modify: `src-tauri/src/remote_im/config.rs` (test mod only — append rotation test)
- Modify: `src-tauri/src/remote_im/outbound.rs` (test mod only — append unregister test)

**Interfaces:**
- Consumes (config.rs): the existing test template — `crate::paths::APP_HOME_ENV_LOCK.lock()`, `guard()`, `test_home("rotate")`, `MockStore`, `install_test_store`/`reset_test_store`, helpers `dto(id)`, `secrets(pairs)`, `save_instance`, `get_secrets`, `read_channel_refs`, `channel_refs_path`, `NS_REMOTE` (all already in scope in the test mod).
- Consumes (outbound.rs): `OutboundRouter::new()`, `register(id, channel, secrets, options)`, `unregister(id)` (outbound.rs:53), `secrets_for_test(id)`.
- Produces: `save_instance_rotates_existing_secret_in_place` (config.rs), `unregister_revokes_router_secrets` (outbound.rs).

- [ ] **Step 1: Write the rotation test (config.rs)**

Append inside `mod tests` in `src-tauri/src/remote_im/config.rs`, after `delete_instance_removes_store_entries_refs_and_legacy_row` (before the final closing `}` of the mod):

```rust
    /// AC-8.7 / AC-8.17: key rotation — saving a new value for an existing
    /// field replaces it in the OS store immediately; the on-disk reference
    /// stays stable (one row per field) and no plaintext of either value
    /// touches disk.
    #[test]
    fn save_instance_rotates_existing_secret_in_place() {
        let _env = crate::paths::APP_HOME_ENV_LOCK.lock().unwrap();
        let _g = guard();
        let tmp = test_home("rotate");
        std::env::set_var("OMP_DESKTOP_HOME", &tmp);
        let mock = Arc::new(MockStore::new());
        install_test_store(mock.clone());

        save_instance(&dto("inst1"), &secrets(&[("bot_token", "tok-v1")])).unwrap();
        assert_eq!(
            get_secrets("inst1").get("bot_token").map(String::as_str),
            Some("tok-v1")
        );

        save_instance(&dto("inst1"), &secrets(&[("bot_token", "tok-v2")])).unwrap();

        // The rotated value resolves immediately through the same reference.
        assert_eq!(
            get_secrets("inst1").get("bot_token").map(String::as_str),
            Some("tok-v2")
        );
        let refs = read_channel_refs(&channel_refs_path());
        let row = refs.get("inst1").expect("refs row for inst1");
        assert_eq!(row.len(), 1);
        assert_eq!(
            row.get("bot_token").map(String::as_str),
            Some("keychain:v1:remote:inst1:bot_token")
        );
        let legacy = tmp.join("remote").join("channel-secrets.json");
        if legacy.is_file() {
            let raw = fs::read_to_string(&legacy).unwrap();
            assert!(!raw.contains("tok-v1"));
            assert!(!raw.contains("tok-v2"));
        }

        reset_test_store();
        std::env::remove_var("OMP_DESKTOP_HOME");
        let _ = fs::remove_dir_all(&tmp);
    }
```

- [ ] **Step 2: Write the revocation test (outbound.rs)**

Append inside `mod tests` in `src-tauri/src/remote_im/outbound.rs`, after the Task 2 tests:

```rust
    /// AC-8.7: revocation — unregister drops the instance's secrets from the
    /// router immediately.
    #[test]
    fn unregister_revokes_router_secrets() {
        let r = OutboundRouter::new();
        let mut secrets = HashMap::new();
        secrets.insert("token".into(), "t".into());
        r.register("inst-9", "weixin", secrets, json!({}));
        assert!(r.secrets_for_test("inst-9").is_some());
        r.unregister("inst-9");
        assert!(r.secrets_for_test("inst-9").is_none());
    }
```

- [ ] **Step 3: Run the tests**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib remote_im:: 2>&1 | grep -E "test result|rotate|revoke"`
Expected: `save_instance_rotates_existing_secret_in_place ... ok`, `unregister_revokes_router_secrets ... ok`, `test result: ok` with 0 failed. On a storm: re-run.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/remote_im/config.rs src-tauri/src/remote_im/outbound.rs
git commit -m "test(remote_im): credential rotation + revocation tests (AC-8.7)"
```

---

### Task 4: AC-8.14 — Discord attachment fetch against a mock HTTP server

**Files:**
- Modify: `src-tauri/src/remote_im/media.rs` (test mod only; the Telegram/Feishu legs stay untested — hardcoded base URLs, spec D4)

**Interfaces:**
- Consumes: `fetch_attachment(channel, secrets, options, att) -> Result<MediaBytes, String>`; `Attachment { kind, source }`, `AttachmentKind::Image`, `AttachmentSource::Discord { url }` (types.rs:39-63). `AttachmentKind` is NOT imported at media.rs top — reference it in tests as `super::super::types::AttachmentKind` or add `use super::types::AttachmentKind;` inside the test mod (preferred).
- Produces: `spawn_one_shot_http(response: Vec<u8>) -> String` test helper + 2 tests.

- [ ] **Step 1: Write the tests**

In `src-tauri/src/remote_im/media.rs`, inside `#[cfg(test)] mod tests`, add after `use super::*;`:

```rust
    use super::super::types::AttachmentKind;
```

Append after `test_media_bytes_construct`:

```rust
    /// Spawn a one-shot HTTP/1.1 server on 127.0.0.1 that replies with the
    /// given raw response bytes; returns the base URL (no trailing slash).
    fn spawn_one_shot_http(response: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            use std::io::{Read, Write};
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf); // consume the request line + headers
            stream.write_all(&response).unwrap();
        });
        format!("http://{addr}")
    }

    fn discord_attachment(url: String) -> Attachment {
        Attachment {
            kind: AttachmentKind::Image,
            source: AttachmentSource::Discord { url },
        }
    }

    /// AC-8.14: Discord fetch leg — Content-Type header wins over the URL
    /// extension; bytes pass through unchanged.
    #[tokio::test]
    async fn fetch_attachment_discord_uses_content_type_header() {
        let body = b"\xff\xd8\xff".to_vec(); // jpeg magic
        let mut resp = b"HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: 3\r\nConnection: close\r\n\r\n".to_vec();
        resp.extend_from_slice(&body);
        let base = spawn_one_shot_http(resp);
        let att = discord_attachment(format!("{base}/cdn/attachments/1/photo.bin"));
        let got = fetch_attachment("discord", &HashMap::new(), &Value::Null, &att)
            .await
            .unwrap();
        assert_eq!(got.data, body);
        assert_eq!(got.mime_type, "image/jpeg");
    }

    /// AC-8.14: Discord fetch leg — without a Content-Type header the MIME
    /// falls back to the URL extension.
    #[tokio::test]
    async fn fetch_attachment_discord_falls_back_to_extension() {
        let body = b"RIFFWEBP".to_vec();
        let mut resp = b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\n".to_vec();
        resp.extend_from_slice(&body);
        let base = spawn_one_shot_http(resp);
        let att = discord_attachment(format!("{base}/cdn/attachments/1/sticker.webp"));
        let got = fetch_attachment("discord", &HashMap::new(), &Value::Null, &att)
            .await
            .unwrap();
        assert_eq!(got.data, body);
        assert_eq!(got.mime_type, "image/webp");
    }
```

- [ ] **Step 2: Run the media tests**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib remote_im::media 2>&1 | tail -5`
Expected: 4 passed (2 existing + 2 new), 0 failed. On a storm: re-run.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/remote_im/media.rs
git commit -m "test(remote_im): discord attachment fetch against mock HTTP server (AC-8.14)"
```

---

### Task 5: Matrix flips + CHANGELOG + full gate sweep + memory

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md` (4 rows + counts row)
- Modify: `CHANGELOG.md` (one bilingual bullet)
- Modify: memory `omp-desktop-roadmap-status.md` + `MEMORY.md` hook (after commit)

**Interfaces:**
- Consumes: Tasks 1-4 committed and green. Exact current row texts are the `old_string` anchors below (verified 2026-07-31).
- Produces: matrix 50/12/95/1 by grep.

- [ ] **Step 1: Flip AC-3.4 (row :77)**

Replace:

```md
| AC-3.4 | ICU parameter/type validation zero errors across all locales | `node scripts/check-i18n-completeness.mjs` | PARTIAL | Key parity + non-empty checks PASS. **Gap:** the script does not validate ICU placeholder (`{name}`) consistency across locales; `interpolate()` uses a simple regex, not ICU MessageFormat. An en key with `{name}` and a zh value missing it would pass. |
```

with:

```md
| AC-3.4 | ICU parameter/type validation zero errors across all locales | `node scripts/check-i18n-completeness.mjs` | PASS | Key parity + non-empty + **`{var}` placeholder-set parity** checks PASS (1889 keys × 3 locales, 2026-07-31; 6 new gate unit tests). Scope note: the app does not use ICU MessageFormat (`interpolate()` is `{var}`-only — docs/i18n-guide.md §3), so placeholder-set parity is the complete validation surface for this codebase. |
```

- [ ] **Step 2: Flip AC-8.3 (row :145)**

Replace:

```md
| AC-8.3 | Identity whitelist enforcement per channel | `cargo test` remote_im suite | PARTIAL | `outbound.rs:232-264` sender_allowed/allow_from_list/allow_from_blocks_enable implemented and enforced (`engine.rs:290`). **Gap:** no dedicated whitelist accept/reject unit test (only hardcoded `allowFrom: "*"` in engine test). |
```

with:

```md
| AC-8.3 | Identity whitelist enforcement per channel | `cargo test` remote_im suite | PASS | `outbound.rs:232-272` sender_allowed/allow_from_list/allow_from_blocks_enable/require_mention enforced at `engine.rs:383` (reject → localized reply + return). 4 dedicated unit tests (2026-07-31): open/fail-closed/explicit-list accept+reject, comma+whitespace parsing, snake_case alias, require_mention default+overrides. |
```

- [ ] **Step 3: Flip AC-8.7 (row :149)**

Replace:

```md
| AC-8.7 | Revocation handling per channel | `cargo test` remote_im suite | PARTIAL | Instance-level revocation works (`mod.rs:249` delete_instance → delete file + `bridge.rs:231` drop_instance_async + `outbound.rs:53` unregister). **Gap:** no key-rotation / in-flight credential revocation test (AC-8.17). |
```

with:

```md
| AC-8.7 | Revocation handling per channel | `cargo test` remote_im suite | PASS | Instance-level revocation (`mod.rs:249` delete_instance → store entries removed, tested) + router `unregister` drops secrets immediately (test). Key rotation (AC-8.17): `save_instance` replaces the OS-store value in place, refs row stays stable, `get_secrets` returns the rotated value immediately (test, 2026-07-31). Boundary: running adapters capture secrets at register time — rotation applies to a running instance on next (re)start. |
```

- [ ] **Step 4: Refresh AC-8.14 evidence (row :181 — verdict STAYS PARTIAL)**

Replace:

```md
| AC-8.14 | Inbound media (image): Feishu/Telegram/Discord | `cargo test` media suite + manual | PARTIAL | `media.rs` has 2 unit tests (mime-from-extension, MediaBytes construct) covering type construction only. No actual image-fetch test for feishu/telegram/discord. **Unblocker:** manual media send + capture. |
```

with:

```md
| AC-8.14 | Inbound media (image): Feishu/Telegram/Discord | `cargo test` media suite + manual | PARTIAL | `media.rs`: 2 type-construction tests + 2 mock-HTTP fetch tests for the Discord leg (Content-Type header honored; extension fallback; 2026-07-31). Telegram/Feishu legs hardcode platform base URLs — untestable without production seams (deferred, YAGNI). **Remaining gap:** manual media send + capture on real channels. **Unblocker:** per-channel smoke. |
```

- [ ] **Step 5: Update the counts row**

Read the counts table (`grep -n "PASS\b" docs/release/1.0-acceptance-matrix.md | head` to locate it), then update PASS 47→50, PARTIAL 15→12, BLOCKED 95 (unchanged), FAIL 1 (unchanged — self-count). Preserve the row's exact format.

- [ ] **Step 6: Verify counts by grep**

Run:
```sh
cd /Users/po1nt9/Github/grok-app-main
for v in PASS PARTIAL BLOCKED FAIL; do printf "%s " "$v"; grep -oE "\| $v \|" docs/release/1.0-acceptance-matrix.md | wc -l; done
```
Expected: `PASS 50`, `PARTIAL 12`, `BLOCKED 95`, `FAIL 1`. If counts differ, find the row whose verdict cell miscounts and fix it — never edit numbers by fiat.

- [ ] **Step 7: CHANGELOG bullet**

In `CHANGELOG.md` under `## [0.3.1-nightly]` → `### Added / 新增`, after the "Guides batch (§12)" bullet, add:

```md
- **Acceptance gap closure (AC-3.4/8.3/8.7/8.14):** the i18n gate now
  enforces `{var}` placeholder-set parity across locales; new remote_im
  tests cover allow_from whitelist accept/reject, credential rotation +
  revocation, and Discord attachment fetch against a mock HTTP server.
  验收差距补齐：i18n 门新增占位符一致性校验；remote_im 新增白名单、
  凭据轮换/吊销与 Discord 附件抓取测试。
```

- [ ] **Step 8: Full gate sweep**

Run (each must be green; on cargo storm re-run once):
```sh
cd /Users/po1nt9/Github/grok-app-main
pnpm check:i18n && pnpm check:brand && pnpm check:provenance && pnpm check:legal && pnpm typecheck
pnpm vitest run 2>&1 | grep -E "Test Files|Tests "
cd src-tauri && cargo test --lib 2>&1 | tail -3
```
Expected: i18n OK 1889×3; brand/provenance/legal/typecheck exit 0; vitest 849+ tests pass (843 + 6 new), 95 files; cargo 511+ passed (506 + 5 new... exact numbers: outbound +5 (Tasks 2+3), config +1, media +2 = 514 total lib tests), 0 failed, 1 ignored.

- [ ] **Step 9: Commit + memory**

```bash
git add docs/release/1.0-acceptance-matrix.md CHANGELOG.md
git commit -m "docs(release): flip AC-3.4/8.3/8.7 to PASS, refresh AC-8.14 evidence (50/12/95/1)"
```

Then update memory `omp-desktop-roadmap-status.md` (add batch bullet with all 6 commit hashes; description尾注 50/12/95/1) and the `MEMORY.md` hook line.

---

## Self-Review Notes

- Spec coverage: D1→Task 1 (gate + tests + guide), D2→Task 2, D3→Task 3, D4→Task 4, D5→Tasks 1/5, D6 honest negatives→matrix cells + guide text in Tasks 1/5, D7 out-of-scope honored (no production changes anywhere).
- Counts arithmetic: 47+3=50 PASS, 15−3=12 PARTIAL (AC-8.14 unchanged), 95/1 unchanged.
- Type consistency: `extractPlaceholders` name used identically in impl + tests; Rust test fns reference only verified signatures (`unregister(&self, &str)` outbound.rs:53; `Attachment{kind, source}` types.rs:39-42; `AttachmentKind::Image` types.rs:46).
- No placeholders: every code step shows complete code; every command shows expected output.
