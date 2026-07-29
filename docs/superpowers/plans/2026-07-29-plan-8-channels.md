# Plan 8: Channels (11 Platforms) — Implementation Plan Outline

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement 11 platform channel adapters (Feishu, Lark, Telegram, Discord, Slack, Matrix, Mattermost, MS Teams, Webhook, Email, WeChat Personal) that bridge external messaging platforms to the OMP Desktop session model, enabling remote IM integration.

**Architecture:** Each channel is a Rust adapter implementing a common `ChannelAdapter` trait (`send`, `receive`, `authenticate`, `list_conversations`). Adapters register with a channel manager (`src-tauri/src/channels/manager.rs`) that routes messages to/from the active OMP session. Platform-specific config is stored in `AppSettings.channels.*`.

**Tech Stack:** Rust (channel adapters + trait), TypeScript (channel config UI), existing `remote_im` infrastructure (see `src-tauri/src/remote_im/`, `src/components/remoteIm/`).

---

## ⚠️ External Dependencies (BLOCKING)

This plan **cannot be executed** until the following external resources are available:

1. **Platform API credentials** — each of the 11 platforms requires developer credentials (OAuth client IDs, bot tokens, API keys). These must be obtained per-platform from the respective developer portals.
2. **Test accounts** — each platform needs at least one test account (sender + receiver) for integration testing.
3. **WeChat Personal protocol decision** — WeChat Personal uses a non-official protocol. Legal/ethical review needed before implementation; may be downgraded to experimental or dropped.
4. **Feishu/Lark regional testing** — Feishu (China) and Lark (International) share code but need separate regional test endpoints.

**Status:** 🚫 Blocked on platform API credentials and test accounts. Outline only.

---

## Scope

- 10 fixed platform adapters + 1 conditional (WeChat Personal)
- Feishu/Lark shared implementation with separate regional testing
- Technical adapter IDs may split to 12 due to regional endpoints
- WeChat Personal is conditional — may be downgraded to experimental

## Platforms

1. **Feishu** (China) — official OpenAPI, bot webhook
2. **Lark** (International) — same code as Feishu, different regional endpoint
3. **Telegram** — Bot API
4. **Discord** — Bot API + Gateway WebSocket
5. **Slack** — Bolt SDK / Web API
6. **Matrix** — Client-Server API (matrix.org)
7. **Mattermost** — REST API + WebSocket
8. **Microsoft Teams** — Bot Framework
9. **Webhook** (generic) — HTTP POST in/out
10. **Email** (IMAP/SMTP) — polling + send
11. **WeChat Personal** (conditional) — non-official protocol, legal review needed

## High-Level Tasks (detailed TDD steps deferred until deps available)

1. **Define `ChannelAdapter` trait** — `src-tauri/src/channels/adapter.rs` with `send()`, `receive()`, `authenticate()`, `list_conversations()`, `get_config_schema()`.
2. **Implement channel manager** — `src-tauri/src/channels/manager.rs` that registers adapters, routes messages, handles auth state.
3. **Implement each platform adapter** — one Rust file per platform under `src-tauri/src/channels/{feishu,telegram,discord,...}.rs`.
4. **Add platform-specific config UI** — extend `src/components/remoteIm/` with per-platform config panels.
5. **Test each adapter with mock endpoints** — mock HTTP/WebSocket servers per platform.
6. **Implement security disclosure for WeChat** — if WeChat Personal is included, document the non-official protocol risks.

## Preparation Work (can be done NOW without external deps)

- Define the `ChannelAdapter` trait and channel manager skeleton in Rust (stubs returning `runtime_unavailable`).
- Add `channels.*` v1 method schemas for listing/configuring channels (`channels.list`, `channels.configure`, `channels.test`).
- Extend `AppSettings` with a `channels: Map<String, ChannelConfig>` field.
- Survey existing `src-tauri/src/remote_im/` code — the research subagent noted that `remote_im` and `mirror` paths are already in `brand-policy.mjs`'s `userVisiblePathPatterns`, suggesting prior channel work exists. Reuse what's there.

## Existing Infrastructure to Reuse

- `src-tauri/src/remote_im/` — existing remote IM code (per brand-policy path patterns)
- `src-tauri/src/mirror/` — existing mirror code (per brand-policy path patterns)
- `src/components/remoteIm/` — existing remote IM UI components
- `src/i18n/messages.ts` — has `settings.remoteIm.channel.*` keys for several platforms (Feishu, Lark, LINE, WPS Agentspace visible in catalog)
