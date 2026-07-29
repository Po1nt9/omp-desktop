# Contributing to OMP Desktop

Thanks for your interest in contributing to **OMP Desktop**. Issues and PRs are welcome.

## Development

```bash
pnpm install
pnpm dev          # Tauri + Vite
```

Frontend only:

```bash
pnpm dev:ui
```

Checks:

```bash
pnpm typecheck
pnpm test
pnpm build:ui
cd src-tauri && cargo test
```

## Workflow

1. Fork the repository and create a branch.
2. Keep changes small and focused.
3. Run `pnpm typecheck`, `pnpm test`, and `cargo test` (in `src-tauri`) locally.
4. User-visible strings go through `src/i18n/messages.ts` (`en` / `zh` same keys).
5. Do not use `window.confirm` / `prompt` / `alert` in product UI — use in-app dialogs.
6. Open a PR describing the motivation, the change, and how you verified it.

## Guidelines

- Product name: **OMP Desktop**.
- Session and settings data live under the App data root.
- Current specs and plans live under [`docs/superpowers/`](./docs/superpowers/).
- Do not commit `node_modules`, `target`, `dist`, local tokens, or `secrets.json`.
- Security-related issues: see [SECURITY.md](./SECURITY.md).

## Contact

- GitHub Issues: <https://github.com/Po1nt9/omp-desktop/issues>

## Releases

1. Write notes under `## [X.Y.Z] - YYYY-MM-DD` in `CHANGELOG.md` (what changed — list form).
2. Commit on a clean `main`.
3. Run `./scripts/release-tag.sh X.Y.Z` (optionally `--push`).
4. CI builds **macOS ARM + Intel + Windows + Linux** and sets the **GitHub Release body** from that CHANGELOG section via `scripts/changelog-for-release.py`.

Do not tag without a matching CHANGELOG section — the release job will fail.

## Attribution

OMP Desktop is adapted from `RongleCat/grok-app` (MIT) at commit
`d2a2563f19bba46cb67496d3b4ac821a31bceaed`. Upstream author: [RongleCat](https://github.com/RongleCat).
