# OMP Desktop Plans 4-10 Roadmap

This document outlines the scope and key tasks for the remaining plans (4-10) in the OMP Desktop 1.0 roadmap. Plans 1-3 are complete.

> **Plan documents:**
> - Plan 4: [`2026-07-29-plan-4-config-provider-mcp-skills-credentials.md`](./2026-07-29-plan-4-config-provider-mcp-skills-credentials.md) — ✅ Complete
> - Plan 5: [`2026-07-29-plan-5-todo-subagent-branch-rewind-attachments-diagnostics.md`](./2026-07-29-plan-5-todo-subagent-branch-rewind-attachments-diagnostics.md) — ✅ Complete
> - Plan 6: [`2026-07-29-plan-6-i18n.md`](./2026-07-29-plan-6-i18n.md) — ✅ Complete
> - Plan 7: [`2026-07-29-plan-7-remote-hub.md`](./2026-07-29-plan-7-remote-hub.md) — ✅ Shipped (as local Runtime Bridge, not a remote Hub)
> - Plan 8: [`2026-07-29-plan-8-channels.md`](./2026-07-29-plan-8-channels.md) — ✅ Shipped (14 adapters)
> - Plan 9: [`2026-07-29-plan-9-os-packaging.md`](./2026-07-29-plan-9-os-packaging.md) — ✅ Complete (OS codesign deferred to optional)
> - Plan 10: [`2026-07-29-plan-10-1.0-acceptance.md`](./2026-07-29-plan-10-1.0-acceptance.md) — 🟡 Ready (codesign no longer blocking)

## Plan 4: Config, Provider, MCP, Skills, and Secure Credentials

**Status:** ✅ Complete
**Depends on:** Plan 3 (complete)
**Spec:** Master design §3 item 4

### Scope
- Active Directory discovery (PI_CODING_AGENT_DIR, OMP_PROFILE)
- Configuration source of truth (config.yml, settings.json, CLI overlays)
- Model catalog (from OMP `@oh-my-pi/pi-catalog`)
- Auth-broker integration (local SQLite + optional remote broker)
- System secure storage migration (from Grok App credential format)
- MCP source registry and discovery
- Skills listing and toggle

### Key Tasks
1. Wire `mcp.list` and `mcp.discover` v1 handlers to real OMP MCP config
2. Wire `diagnostics.selfCheck` v1 handler to real OMP diagnostics
3. Wire `credentials.*` v1 handlers to real `AuthStorage` (with credential migration status)
4. Implement config discovery and source-of-truth resolution in Desktop host
5. Add credential migration from legacy Grok App format (if applicable)
6. Add MCP source management UI commands

### Estimated Complexity: Medium-High
### External Dependencies: None (all local)

---

## Plan 5: Todo, Subagent, Branch, Rewind, Attachments, Diagnostics

**Status:** ✅ Complete
**Depends on:** Plan 4 (for MCP/diagnostics wiring)
**Spec:** Master design §3 item 5

### Scope
- Todo list integration (via OMP tool layer)
- Subagent management (spawn, monitor, collect results)
- Session branching (create branch from any point)
- Session rewind (drop last user turn, restore to checkpoint)
- Attachment handling (images, files, clipboard paste)
- Diagnostic self-check and export

### Key Tasks
1. Add Tauri commands for todo list CRUD
2. Add subagent spawn/monitor Tauri commands
3. Implement session branching via OMP `forkSession`
4. Implement rewind via local journal truncation (already partially done in Plan 3 Task 7)
5. Wire attachment paths to session media directories
6. Implement diagnostic bundle export

### Estimated Complexity: Medium
### External Dependencies: None

---

## Plan 6: i18n (en, zh, zh-TW)

**Status:** ✅ Complete
**Depends on:** Plan 2 (for protocol layer)
**Spec:** Master design §3 item 6, §14

### Scope
- Three locale support: English (source), Simplified Chinese (canonical id `zh`; `zh-CN`/`zh-Hans` aliases accepted), Traditional Chinese (`zh-TW`)
- Static validation ensuring all three locales have complete coverage
- Localized Runtime envelope (agentInfo.title, error messages, mode names)
- Native surface localization (menu, tray, notifications, dialogs)
- Brand normalization (OMP / OMP Runtime in all locales)

### Key Tasks
1. Extract all user-visible strings from React components
2. Locale files for `en`, `zh`, `zh-TW` (already in place; `zh-CN` accepted as alias by `resolveLocale` and `Locale::parse`)
3. Add i18n provider and locale switcher
4. Localize Tauri tray menu, notifications, and dialogs
5. Add brand normalization for Runtime metadata
6. Add static validation script that checks locale completeness
7. Localize error messages from v1 protocol

### Estimated Complexity: Medium
### External Dependencies: None

---

## Plan 7: Remote Hub

**Status:** ✅ Shipped as the **Remote IM Runtime Bridge** (`v0.3.0-nightly`).
**Depends on:** Plan 3 (for runtime and session model)
**Spec:** Master design §3 item 7, §16

### Scope
- Remote Hub architecture for cross-device session sync
- Session sharing and remote approval
- Security model: OMP permission results, no extra PIN
- Chat account compromise risk disclosure

### Key Tasks
1. Design Remote Hub protocol (extends v1 protocol)
2. Implement Hub client in Desktop host
3. Add remote approval routing through OMP permission system
4. Implement session sync (journal replay)
5. Add security disclosure UI

### Estimated Complexity: High
### External Dependencies: None — the Hub was dropped (see below).

### How it actually shipped (deviation from original design)

The Remote Hub server was **not built**. A pre-implementation review concluded the
Hub bundled three concerns, two of which (network reachability, multi-channel
orchestration) do not need a central server — the 14 channel adapters use
outbound long connections (WebSocket / long-poll / Socket Mode), proven by
[cc-connect](https://github.com/chenhg5/cc-connect) and
[hermes-agent](https://github.com/NousResearch/hermes-agent). The third concern,
cross-device session sync, is not a 1.0 requirement and was deferred.

What shipped instead (the `remote_im` engine's fail-closed gates replaced with
real OMP Runtime calls):
- **Per-`work_dir` AcpClient pool** — one runtime client per agent working dir.
- **Drain barrier** — in-flight turns complete before shutdown.
- **3-layer concurrency locks** — per-channel, per-session, per-runtime.
- Inbound IM messages drive real agent turns (end-to-end in `v0.3.0-nightly`).

See `CHANGELOG.md` `[0.3.0-nightly]` → "Remote IM Runtime Bridge (Plan 7)".
The originally-scoped **cross-device session sync** remains a possible future
plan (a lightweight journal relay over Plan 3's event journal), not blocking 1.0.

---

## Plan 8: Channels (11 Platforms)

**Status:** ✅ Shipped as **14 channel adapters** (`v0.3.0-nightly`).
**Depends on:** Plan 7 (shipped — the Runtime Bridge, not the Hub)
**Spec:** Master design §3 item 8, §17

### Scope
- 10 fixed platform adapters + 1 conditional (WeChat personal)
- Feishu/Lark shared implementation with separate regional testing
- Technical adapter IDs may split to 12 due to regional endpoints
- WeChat personal is conditional — may be downgraded to experimental

### Platforms
1. Feishu (China)
2. Lark (International)
3. Telegram
4. Discord
5. Slack
6. Matrix
7. Mattermost
8. Microsoft Teams
9. Webhook (generic)
10. Email (IMAP/SMTP)
11. WeChat Personal (conditional — non-official protocol)

### Key Tasks
1. Define channel adapter interface
2. Implement each platform adapter
3. Add platform-specific configuration UI
4. Test each adapter with mock endpoints
5. Implement security disclosure for WeChat

### Estimated Complexity: Very High
### External Dependencies: None (outbound connections; no server infra).

### How it actually shipped (deviation from original design)

14 adapters shipped in `v0.3.0-nightly`: feishu, telegram, discord, slack,
dingtalk, wecom, weixin, qq, qqbot, matrix, line, weibo, wpc_xiezuo, generic.

The original plan listed 11 platforms; the delta comes from regional splits
(e.g. Feishu vs Lark share an implementation but were enumerated separately)
and the addition of a generic adapter. WeChat Personal and Email/Webhook were
**not** in the shipped set; they remain possible future additions. Because the
Hub was dropped (see Plan 7), these adapters connect via outbound long
connections with zero server infrastructure.

---

## Plan 9: OS Packaging and Updates

**Status:** ✅ Complete. Packaging pipeline builds all four targets and publishes
installers + SHA256SUMS. **Updater signing is enabled** (minisign keypair,
`v0.3.1-nightly`) so in-app silent update works. **Community distribution
channels are live**: Homebrew Cask (macOS), one-line `curl | bash` installer
(macOS/Linux). **OS code-signing deferred to optional**: macOS Developer ID
notarization and Windows Authenticode require purchasing certificates
(Apple $99/yr; Windows OV/EV $100-700/yr) — free alternatives documented
(SignPath Foundation for Windows; Homebrew/xattr for macOS). These are
user-experience polish, not release blockers; existing workarounds (Homebrew
quarantine bypass, `xattr -cr`, SmartScreen "Run anyway") are sufficient for
the target developer audience.
**Depends on:** Plans 3-8 (for complete feature set)
**Spec:** Master design §3 item 9, §20

### Scope
- macOS Universal build (Intel + ARM)
- Windows x64 build (ARM64 deferred post-1.0)
- Linux build (AppImage + .deb + .rpm)
- Code signing for macOS and Windows
- Auto-update channels: stable, beta, nightly
- OMP Runtime bundling and co-signing

### Key Tasks
1. Configure Tauri build for each platform
2. Bundle OMP Runtime binary in app resources
3. Set up code signing (macOS notarization, Windows authenticode)
4. Configure update channels with isolated config/cache/signing
5. Test update flow (stable → beta → nightly)
6. Create installer packages for each platform

### Estimated Complexity: Very High
### External Dependencies: Code signing certificates (Apple/Windows) + notarization service — updater signing needs none.

### What's done vs. what's left

- ✅ Cross-platform build pipeline (macOS ARM/x64, Windows x64, Linux x64).
- ✅ Installer formats (DMG, NSIS + portable zip, AppImage, .deb, .rpm) + SHA256SUMS.
- ✅ Updater artifacts + `latest.json` (signed, `v0.3.1-nightly`).
- ✅ Graceful degradation when signing secrets are absent.
- ✅ Homebrew Cask tap ([Po1nt9/homebrew-tap](https://github.com/Po1nt9/homebrew-tap)) — macOS install without Gatekeeper dialog.
- ✅ One-line installer script (`scripts/install.sh`) — macOS `xattr -cr` + Linux AppImage.
- ⬜ macOS notarization (optional — needs Apple Developer ID cert; Homebrew/xattr workaround in place).
- ⬜ Windows Authenticode (optional — needs OV/EV cert or free SignPath Foundation; SmartScreen bypass available).
- ⬜ Winget / Scoop submission (optional — deferred to first stable release).

---

## Plan 10: 1.0 Acceptance Matrix

**Status:** 🟡 Ready. OS code-signing is no longer a blocker (deferred to
optional). Remaining deps: cross-platform testing infrastructure + performance
benchmark baselines + security auditor.
**Depends on:** Plans 1-9 (Plan 9 is partial: updater signed, OS codesign pending)
**Spec:** Master design §3 item 10, §5

### Scope
- Capability baseline 100% coverage verification
- Cross-platform acceptance testing
- Performance benchmarks
- Security audit
- Documentation review
- Release readiness sign-off

### Key Tasks
1. Create acceptance test matrix covering all capability baseline items
2. Run acceptance tests on macOS, Windows, Linux
3. Performance benchmarks (startup time, memory, response latency)
4. Security audit (credential storage, permission model, remote access)
5. Documentation review (user guide, admin guide, troubleshooting)
6. Release readiness sign-off

### Estimated Complexity: Medium (but blocking)
### External Dependencies: Testing infrastructure on all platforms

---

## Summary

| Plan | Status | Complexity | External Deps |
|---|---|---|---|
| 1. Repository & Brand Baseline | ✅ Complete | - | - |
| 2. Extension Protocol | ✅ Complete | - | - |
| 3. Supervisor, Core ACP, Event Journal | ✅ Complete | - | - |
| 4. Config, Provider, MCP, Skills, Credentials | ✅ Complete | Medium-High | None |
| 5. Todo, Subagent, Branch, Rewind, Attachments | ✅ Complete | Medium | None |
| 6. i18n | ✅ Complete | Medium | None |
| 7. Remote Hub | ✅ Shipped (Runtime Bridge) | High | None (Hub dropped) |
| 8. Channels | ✅ Shipped (14 adapters) | Very High | None |
| 9. OS Packaging | ✅ Complete (codesign optional) | Very High | None (certs optional) |
| 10. 1.0 Acceptance | 🟡 Ready | Medium | Test infra + benchmarks + auditor |

Plans 1-9 are complete. Plan 9's OS code-signing (macOS notarization, Windows
Authenticode, Winget/Scoop) is deferred to optional — existing workarounds
(Homebrew, xattr, SmartScreen bypass) suffice for the developer audience.
Plan 10 (1.0 acceptance) can proceed; remaining deps are cross-platform testing
infrastructure, performance benchmark baselines, and a security auditor.
