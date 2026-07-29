# OMP Desktop Plans 4-10 Roadmap

This document outlines the scope and key tasks for the remaining plans (4-10) in the OMP Desktop 1.0 roadmap. Plans 1-3 are complete.

## Plan 4: Config, Provider, MCP, Skills, and Secure Credentials

**Status:** Not started
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

**Status:** Not started
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

## Plan 6: i18n (en, zh-CN, zh-TW)

**Status:** Not started
**Depends on:** Plan 2 (for protocol layer)
**Spec:** Master design §3 item 6, §14

### Scope
- Three locale support: English (source), Simplified Chinese, Traditional Chinese
- Static validation ensuring all three locales have complete coverage
- Localized Runtime envelope (agentInfo.title, error messages, mode names)
- Native surface localization (menu, tray, notifications, dialogs)
- Brand normalization (OMP / OMP Runtime in all locales)

### Key Tasks
1. Extract all user-visible strings from React components
2. Create locale files for `en`, `zh-CN`, `zh-TW`
3. Add i18n provider and locale switcher
4. Localize Tauri tray menu, notifications, and dialogs
5. Add brand normalization for Runtime metadata
6. Add static validation script that checks locale completeness
7. Localize error messages from v1 protocol

### Estimated Complexity: Medium
### External Dependencies: None

---

## Plan 7: Remote Hub

**Status:** Not started
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
### External Dependencies: Remote Hub server infrastructure (not available in 1.0 scope)

---

## Plan 8: Channels (11 Platforms)

**Status:** Not started
**Depends on:** Plan 7 (for Remote Hub)
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
### External Dependencies: Platform API credentials, test accounts

---

## Plan 9: OS Packaging and Updates

**Status:** Not started
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
### External Dependencies: Code signing certificates, CI/CD infrastructure, notarization service

---

## Plan 10: 1.0 Acceptance Matrix

**Status:** Not started
**Depends on:** Plans 1-9 (all complete)
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
| 4. Config, Provider, MCP, Skills, Credentials | 📋 Not started | Medium-High | None |
| 5. Todo, Subagent, Branch, Rewind, Attachments | 📋 Not started | Medium | None |
| 6. i18n | 📋 Not started | Medium | None |
| 7. Remote Hub | 📋 Not started | High | Server infra |
| 8. Channels | 📋 Not started | Very High | Platform APIs |
| 9. OS Packaging | 📋 Not started | Very High | Signing certs |
| 10. 1.0 Acceptance | 📋 Not started | Medium | All platforms |

Plans 4-6 can be executed immediately with no external dependencies. Plans 7-10 require external infrastructure and should be prioritized based on available resources.
