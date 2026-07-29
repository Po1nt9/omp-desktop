# Plan 1 Repository and Brand Baseline Verification

- Grok App baseline: d2a2563f19bba46cb67496d3b4ac821a31bceaed
- Frozen design ancestor: 05e2aaa96bbdeb68d9740d102ccb1723e26673ed
- OMP source baseline: 667111575ebba136dadfd6989379e7f67e0d40d9
- Runtime behavior: fail closed (`runtime_unavailable`)
- Brand policy: zero violations
- Provenance policy: passed
- Legal/SBOM input policy: passed
- Frontend typecheck/tests/build: passed
- Rust tests/metadata: passed
- Sidecar bundled: no; scheduled for later plans

## Verification environment

- Platform: macOS (Darwin)
- Date: 2026-07-29

## Step 1: Dependency reinitialization

- `pnpm install --frozen-lockfile`: succeeded without lockfile mutation
- `git submodule update --init --recursive`: succeeded
- Submodule HEAD: `667111575ebba136dadfd6989379e7f67e0d40d9` (matches pinned commit)

## Step 2: Custom policy gates

- `pnpm check:provenance`: passed — frozen records, remotes, gitlink, and submodule checkout match
- `pnpm check:brand`: passed — zero violations
- `pnpm check:legal`: passed — inventory, policy, and notice coverage verified
- `node --test scripts/check-provenance.test.mjs scripts/check-brand-policy.test.mjs scripts/check-legal-baseline.test.mjs`: 24 tests passed, 0 failed

## Step 3: Frontend verification

- `pnpm typecheck`: zero TypeScript errors
- `pnpm test`: 93 test files, 822 tests passed, 0 failed
- `pnpm build:ui`: Vite build succeeded, `dist/` produced

## Step 4: Rust verification

- `cargo test --manifest-path src-tauri/Cargo.toml --locked`: 352 tests passed, 0 failed, 1 ignored
- `cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1`: exited 0

## Step 5: Removed runtime paths

- `git grep -nE '_x\.ai/|GROK_HOME|GROK_APP_ACP|grok_agent_stdio|com\.grokapp\.desktop'`: zero matches in production code (excluding allowlisted scanner fixtures)
- Provider fixture `testdata/brand-policy/allowed/provider-xai.json`: `provider.id === "xai"`
- Model fixture `testdata/brand-policy/allowed/model-grok.json`: `models[0].id === "grok-4.5"`

## Step 6: History, remotes, and tree cleanliness

- `git merge-base --is-ancestor d2a2563f19bba46cb67496d3b4ac821a31bceaed HEAD`: passed
- `git merge-base --is-ancestor 05e2aaa96bbdeb68d9740d102ccb1723e26673ed HEAD`: passed
- `git submodule status --recursive`: `667111575e... runtime/oh-my-pi` (clean, pinned)
- Working tree: clean except for this verification record

## Plan 1 completion

Plan 1 is complete. The repository is a correctly branded, reproducible, buildable, fail-closed OMP Desktop shell with the OMP source pinned and all Grok App-specific runtime behavior removed. Plan 2 may define and implement the versioned OMP Desktop Extension Protocol without inheriting hidden Grok assumptions.
