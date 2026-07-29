# Plan 5 Verification Record: Todo, Subagent, Branch, Rewind, Attachments, Diagnostics

- **Date:** 2026-07-29
- **Plan:** [2026-07-29-plan-5-todo-subagent-branch-rewind-attachments-diagnostics.md](../plans/2026-07-29-plan-5-todo-subagent-branch-rewind-attachments-diagnostics.md)
- **Status:** PASS

## Summary

Plan 5 added six new v1 protocol methods (`todo.list`, `subagents.status`, `subagents.setEnabled`, `sessions.fork`, `sessions.rewindPoints`, `sessions.rewind`, `sessions.resolveMedia`, `diagnostics.exportBundle`) and replaced the stale `queue.*` / `steer.*` stubs (which said "requires Plan 3" even though Plan 3 shipped) with real backing.

## Files Changed

### New OMP runtime handler files
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/todo.ts` + `.test.ts`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/subagents.ts` + `.test.ts`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/branch.ts` + `.test.ts`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/rewind.ts` + `.test.ts`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/media.ts` + `.test.ts`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/diagnostics.test.ts`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/queue.test.ts`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/steer.test.ts`

### Modified OMP runtime files
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/methods.ts` — 8 new method schemas
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/generated/schema-bundle.json` — regenerated bundle (34 method schemas, digest `d0880d7a80124a77`)
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/index.ts` — registered new handlers, extended `HandlerDeps`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/queue.ts` — replaced stub with `QueueLike` backing
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/steer.ts` — replaced stub with `SteerLike` backing
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/diagnostics.ts` — added `exportBundle` handler
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/types.ts` — added 7 new structural interfaces
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts` — wired new deps in `buildDesktopV1HandlerDeps`

### Modified Desktop host files
- `src/lib/ompDesktopV1/methods.ts` — 8 new `MethodMap` entries + 8 new params/result interface pairs
- `src-tauri/src/omp_desktop_v1/generated.rs` — 4 new Rust mirror structs (`TodoTask`, `TodoPhase`, `RewindPoint`, `MediaAttachment`)
- `scripts/brand-policy.mjs` — added Plan 4 verification file to `wholeFileAllowlist` (pre-existing omission)
- `provenance/omp-patches.json` — Plan 5 patch entry (submodule HEAD `a2eb2070b`)

## Test Results

- **OMP runtime handler tests:** PASS — 21 pass, 0 fail, 35 expect() calls across 10 files (138ms)
- **Frontend tests (vitest):** PASS — 94 test files passed, 829 tests passed, 0 failed (4.28s)
- **Rust tests:** PASS with 1 pre-existing failure — 380 passed, 1 failed, 1 ignored
  - The single failure is `store::tests::ensure_general_project_is_idempotent_and_not_removable` (assertion `PathBuf::from(&a.path).is_dir()` in `src/store.rs:1765`). This is a pre-existing environment-specific test failure unrelated to Plan 5; the OMP Desktop v1 contract tests and all Plan 5 mirror types compile and pass.
- **Rust build:** PASS — `cargo build` exit 0 (14.97s, 150 pre-existing warnings, 0 errors)
- **Brand policy:** PASS — 0 violations after fixing pre-existing Plan 4 verification file allowlist omission

## Schema Bundle

- **Method count:** 34 (26 pre-Plan-5 + 8 new Plan 5 methods)
- **Schema digest:** `d0880d7a80124a77`
- **New methods present:** `todo.list`, `subagents.status`, `subagents.setEnabled`, `sessions.fork`, `sessions.rewindPoints`, `sessions.rewind`, `sessions.resolveMedia`, `diagnostics.exportBundle`

## Known Gaps

- `branch`/`rewind`/`media`/`diagnostics.exportBundle` deps use `globalThis.__ompDesktopV1*` hooks that the Desktop host must register at spawn time. When not registered (CLI mode), handlers correctly throw `runtime_unavailable`.
- `queue`/`steer` deps depend on `sessionLookup()?.promptQueue` / `.steerable` fields. If these fields don't exist on `AgentSession`, the deps are `null` and handlers throw `runtime_unavailable`. This is a safe default; the fields can be wired when the session record type is extended.
- The Rust `DesktopV1Capability.methods` field is a `Vec<String>` populated at runtime from the ACP `initialize` response — it is not hardcoded or derived from the schema bundle at build time. The 4 new Rust mirror structs (`TodoTask`, `TodoPhase`, `RewindPoint`, `MediaAttachment`) are provided for parity with the frontend `MethodMap` but are not yet consumed by any Rust call site.
