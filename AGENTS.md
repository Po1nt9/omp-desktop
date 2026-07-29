# Agent notes — OMP Desktop

## Read first

1. **`docs/superpowers/`** — current OMP Desktop specs and plans.
   - [2026-07-28-omp-desktop-design.md](docs/superpowers/specs/2026-07-28-omp-desktop-design.md) — frozen master design
   - [2026-07-28-repository-brand-baseline.md](docs/superpowers/plans/2026-07-28-repository-brand-baseline.md) — repository brand baseline plan
2. **Plan 1 baseline is fail-closed.** Agent execution, Provider authentication, and runtime-owned configuration return `runtime_unavailable`. Do not advertise these as working capabilities.
3. The OMP Runtime source is pinned as a submodule at `runtime/oh-my-pi` (commit `667111575ebba136dadfd6989379e7f67e0d40d9`).
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
