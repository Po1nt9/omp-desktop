# Plan 2 OMP Desktop Extension Protocol Verification

- OMP submodule base: 667111575ebba136dadfd6989379e7f67e0d40d9
- OMP submodule patched: 1333fd28c4f283c9960f0a8030ac1fcad13f3802
- Patch branch: desktop-v1-protocol
- Namespace: _omp/desktop/v1/*
- Methods defined: 24
- Notifications defined: 4
- Error codes: 9
- Legacy compat: 6 methods mapped
- Schema digest: 52190b013ce80537
- Brand policy: zero violations
- Provenance policy: passed (patch recorded)
- Legal/SBOM input policy: passed
- OMP submodule tests: passed (desktop-v1 + acp-agent)
- Frontend typecheck/tests/build: passed
- Rust tests/metadata: passed
- Dead x.ai/rewind bindings: removed
- Stale lib.rs doc: fixed
- Capability negotiation: implemented (gated on OMP_DESKTOP_V1_PROTOCOL=1)
- Queue/steer handlers: return runtime_unavailable (Plan 3 dependency)

## Verification environment

- Platform: macOS (Darwin)
- Date: 2026-07-29

## Step 1: Dependency reinitialization

- `pnpm install --frozen-lockfile`: succeeded without lockfile mutation
- `git submodule update --init --recursive`: succeeded (no-op, submodule already at patched commit)
- Submodule HEAD: `1333fd28c4f283c9960f0a8030ac1fcad13f3802` (matches patch commit)

## Step 2: Custom policy gates

- `pnpm check:provenance`: passed — patch entry validated, gitlink and submodule checkout match patched commit
- `pnpm check:brand`: passed — zero violations (Plan 2 plan and Plan 1 verification added to wholeFileAllowlist for legitimate legacy references in documentation)
- `pnpm check:legal`: passed — inventory, policy, and notice coverage verified
- `node --test scripts/check-provenance.test.mjs scripts/check-brand-policy.test.mjs scripts/check-legal-baseline.test.mjs`: 27 tests passed, 0 failed

## Step 3: OMP submodule tests

- `OMP_DESKTOP_V1_PROTOCOL=1 bun test packages/coding-agent/test/desktop-v1/`: 92 tests passed, 0 failed (12 files)
- `bun test packages/coding-agent/test/acp-agent.test.ts`: 54 tests passed, 0 failed

## Step 4: Frontend verification

- `pnpm typecheck`: zero TypeScript errors
- `pnpm test`: 94 test files, 829 tests passed, 0 failed
- `pnpm build:ui`: Vite build succeeded, `dist/` produced

## Step 5: Rust verification

- `cargo test --manifest-path src-tauri/Cargo.toml --locked`: 360 tests passed, 0 failed, 1 ignored
- `cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1`: exited 0

## Step 6: Removed private bindings

- `git grep -nE '_x\.ai/|x\.ai/rewind' -- src src-tauri`: zero matches (grep exit 1)
- Rust regression test `no_private_xai_bindings_remain`: passed
- `acp_client.rs` header: clean (no private extension or rewind literals)
- `lib.rs` module doc: `OMP Desktop Host — Tauri application entrypoint.` (stale `grok agent stdio` removed)

## Step 7: Provenance patch

- Patch recorded in `provenance/omp-patches.json` with commit `1333fd28c4f283c9960f0a8030ac1fcad13f3802`
- `scripts/check-provenance.mjs` updated to validate patch entries and compute expected submodule commit from the last recorded patch
- Base commit ancestor check ensures the patch is built on top of the frozen base

## Plan 2 completion

Plan 2 is complete. The OMP Desktop Extension Protocol v1 defines a versioned `_omp/desktop/v1/*` namespace with 24 methods, 4 notifications, 9 error codes, and 6 legacy compat mappings. The schema digest `52190b013ce80537` is negotiated during ACP `initialize` when `OMP_DESKTOP_V1_PROTOCOL=1`. The Rust `OmpExtension` client and frontend typed client provide fail-closed behavior until Plan 3 wires the real Supervisor and ACP transport. Queue and steer handlers return `runtime_unavailable` as Plan 3 dependencies.
