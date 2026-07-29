# Plan 7: Remote Hub — Implementation Plan Outline

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable cross-device session sync, remote approval routing, and session sharing through a Remote Hub server, extending the OMP Desktop v1 protocol with a `_omp/desktop/v1/remote/*` namespace.

**Architecture:** A Remote Hub server (separate infrastructure, not bundled with the desktop app) acts as a relay between desktop instances. The desktop app connects as a Hub client; session journals (from Plan 3) are replayed to remote peers. Remote approvals route through the existing OMP permission system (no extra PIN — see security model below).

**Tech Stack:** TypeScript (frontend Hub client), Rust (Tauri-side transport), existing v1 protocol + event journal.

---

## ⚠️ External Dependencies (BLOCKING)

This plan **cannot be executed** until the following external resources are available:

1. **Remote Hub server infrastructure** — a deployed server that relays session journals between desktop clients. No server implementation exists in this repository; the desktop app is a client only.
2. **Hub authentication model** — design decision needed: OAuth, device pairing, or shared secret. Currently unspecified.
3. **Hub server reference implementation** — needed for integration testing. Even a mock server would unblock Tasks 1-4.

**Status:** 🚫 Blocked on external infrastructure. Outline only.

---

## Scope

- Remote Hub protocol design (extends `_omp/desktop/v1/*` with `remote.*` namespace)
- Hub client in Desktop host (connect, authenticate, sync)
- Remote approval routing through OMP permission system (no extra PIN)
- Session sync via journal replay (Plan 3 event journal)
- Security disclosure UI (chat account compromise risk)

## High-Level Tasks (detailed TDD steps deferred until deps available)

1. **Design Remote Hub protocol** — add `remote.connect`, `remote.sync`, `remote.approve`, `remote.disconnect` methods to v1 schema. Define auth handshake.
2. **Implement Hub client in Rust** — `src-tauri/src/remote_hub/client.rs` with WebSocket/HTTP transport to Hub server.
3. **Wire Hub client into Tauri commands** — `remote_hub_connect`, `remote_hub_disconnect`, `remote_hub_status`.
4. **Route remote approvals through OMP permission system** — extend `permission_rules.rs` to accept remote-originated permission requests with the same fail-closed policy.
5. **Implement session sync via journal replay** — use Plan 3's event journal; replay `evt_*` events to remote peers.
6. **Add security disclosure UI** — Settings page section explaining that Remote Hub sync exposes the chat account to compromise if the Hub server is breached.
7. **Tests** — integration tests with mock Hub server; unit tests for protocol schema.

## Preparation Work (can be done NOW without external deps)

- Define the `remote.*` v1 method schemas in `runtime/oh-my-pi/.../desktop-v1/schema/methods.ts` (schema-only, handlers stubbed with `runtime_unavailable`).
- Add frontend `MethodMap` entries for `remote.*` methods in `src/lib/ompDesktopV1/methods.ts`.
- Add Rust types in `src-tauri/src/omp_desktop_v1/generated.rs`.
- Write a security disclosure document (`docs/security/remote-hub-risks.md`) covering chat account compromise scenarios.

These preparation tasks mirror the Plan 2 pattern (define schema + stubs first, wire backing later).
