# Agent notes — OMP Desktop

## Read first

1. **`docs/superpowers/`** — current OMP Desktop specs and plans.
   - [2026-07-28-omp-desktop-design.md](docs/superpowers/specs/2026-07-28-omp-desktop-design.md) — frozen master design
   - [2026-07-29-plans-4-10-roadmap.md](docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md) — Plans 1-10 status tracker
2. **`docs/release/`** — 1.0 release authority: [acceptance matrix](docs/release/1.0-acceptance-matrix.md), [security audit checklist](docs/release/security-audit-checklist.md), [test coverage audit](docs/release/test-coverage-audit.md). Plan 10 Phase 1 (preparation work) is complete; all items are `PENDING` execution.
3. **Frontend/core is fail-closed without a Runtime.** UI boot (`App.fail-closed.test.tsx`) and core ACP surface `runtime_unavailable` when no Runtime is configured. The `remote_im` engine (Plan 7 Runtime Bridge) calls the real Runtime once `binary_path`/`agent_dir` are set in Settings. Do not advertise Agent execution, Provider auth, or remote channels as working without a configured Runtime.
3. The OMP Runtime source is pinned as a submodule at `runtime/oh-my-pi` (commit `64db4c38a1b570efc8a2085e65d86e3ae23e4ef2`). Verify with `git submodule status` before relying on it.
4. Historical upstream material lives under `docs/upstream-history/grok-app/` and does **not** describe the current product.

## Development

```bash
pnpm install
pnpm dev          # Tauri + Vite
pnpm dev:ui       # frontend only
pnpm typecheck
pnpm test
cd src-tauri && cargo test
pnpm build
```

## Conventions

- Product name: **OMP Desktop**.
- README.md is **Chinese** (primary); README_EN.md is English. Keep both in sync when editing install/docs sections.
- Distribution: Homebrew Cask ([Po1nt9/homebrew-tap](https://github.com/Po1nt9/homebrew-tap)), one-line `scripts/install.sh` (macOS/Linux), manual download (Windows). See `docs/release/signing-requirements.md` § Community distribution channels.
- Do not hardcode user-facing English/Chinese. Use `createT(locale)` / `t()` via `src/i18n/`.
- Never use `window.confirm` / `window.prompt` / `window.alert` in Tauri UI. Use App `setAppDialog`, `GlassModal`, or in-app portals.
- Assistant messages render markdown (`MarkdownBody`); user messages use a gray bubble with no role labels.
- Do not commit secrets, `secrets.json`, or local configuration files.
- Security-related issues: see [SECURITY.md](./SECURITY.md).

## Brand policy

The brand scanner (`scripts/check-brand-policy.mjs`) rejects legacy product names, identifiers, runtime env vars, direct xAI endpoints, and lowercase `omp` in user-visible paths. Run `pnpm check:brand` before committing. See `scripts/brand-policy.mjs` for the rule set.

## Branch hygiene

After work lands on `main` (merge, squash, or batch integrate), promptly and safely delete finished remote/local branches and idle worktrees. Confirm with `git fetch --prune`; never delete open-PR heads, unique WIP, or worktree-checked-out branches without removing the worktree first.

## Attribution

OMP Desktop is adapted from `RongleCat/grok-app` (MIT) at commit `d2a2563f19bba46cb67496d3b4ac821a31bceaed`. Upstream author: [RongleCat](https://github.com/RongleCat).
