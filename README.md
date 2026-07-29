<p align="center">
  <img src="assets/logo.png" alt="OMP Desktop" width="128" height="128" />
</p>

<h1 align="center">OMP Desktop</h1>

<p align="center"><strong>Open-source Tauri/React desktop shell adapted to the OMP Runtime</strong></p>

<p align="center">
  <a href="./README.md">English</a> ·
  <a href="./README_ZH.md">中文</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License" /></a>
  <a href="https://github.com/Po1nt9/omp-desktop/stargazers"><img src="https://img.shields.io/github/stars/Po1nt9/omp-desktop?style=social" alt="GitHub stars" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="Platforms" />
  <img src="https://img.shields.io/badge/Tauri-2-orange" alt="Tauri 2" />
</p>

---

> [!NOTE]
> OMP Desktop is an open-source Tauri/React desktop shell being adapted to the OMP Runtime.
> The Plan 1 baseline is intentionally fail-closed: Agent execution, Provider authentication,
> and runtime-owned configuration are unavailable until the versioned OMP integration lands.

---

## Contents

1. [Overview](#overview)
2. [Status](#status)
3. [Development](#development)
4. [Platforms](#platforms)
5. [Provenance](#provenance)
6. [Documentation](#documentation)
7. [Contributing](#contributing)
8. [License](#license)

---

## Overview

OMP Desktop is a Tauri 2 + React + TypeScript desktop shell. It is being adapted from
the upstream `RongleCat/grok-app` project (MIT) into a host for the OMP Runtime.

**Stack:** Tauri 2 + Rust · React + TypeScript + Vite · Tailwind CSS

The OMP Runtime source is pinned as a git submodule at `runtime/oh-my-pi`.

## Status

The Plan 1 baseline is intentionally fail-closed. The following surfaces return
`runtime_unavailable` and are not yet wired to a live runtime:

- Agent execution (no prompts run)
- Provider authentication (no sign-in)
- Runtime-owned configuration (no model catalog, no fallback model)

Do not advertise or document these as working capabilities yet. The frozen master
design lives at
[`docs/superpowers/specs/2026-07-28-omp-desktop-design.md`](./docs/superpowers/specs/2026-07-28-omp-desktop-design.md).

## Development

Requirements: Node 22+, pnpm 9, Rust stable, Xcode CLT (macOS).

```bash
pnpm install

pnpm dev          # full app (Tauri + Vite)
pnpm dev:ui       # frontend only

pnpm typecheck
pnpm test
cd src-tauri && cargo test

pnpm build
```

## Platforms

OMP Desktop targets three operating systems:

| Platform | Target |
|----------|--------|
| macOS | Apple Silicon + Intel |
| Windows | x64 |
| Linux | x64 (AppImage / deb / rpm) |

## Provenance

OMP Desktop is adapted from upstream sources under MIT license:

- **Desktop shell baseline:** `RongleCat/grok-app` at commit `d2a2563f19bba46cb67496d3b4ac821a31bceaed`
- **OMP Runtime submodule:** `runtime/oh-my-pi` at commit `667111575ebba136dadfd6989379e7f67e0d40d9`

Historical upstream material (plans, wikis, acceptance docs) is preserved under
[`docs/upstream-history/grok-app/`](./docs/upstream-history/grok-app/) for provenance.
Those files do not describe the current OMP Desktop product.

## Documentation

| Audience | Link |
|----------|------|
| Frozen master design | [`docs/superpowers/specs/2026-07-28-omp-desktop-design.md`](./docs/superpowers/specs/2026-07-28-omp-desktop-design.md) |
| Brand baseline plan | [`docs/superpowers/plans/2026-07-28-repository-brand-baseline.md`](./docs/superpowers/plans/2026-07-28-repository-brand-baseline.md) |
| Changelog | [`CHANGELOG.md`](./CHANGELOG.md) |
| Contributing | [`CONTRIBUTING.md`](./CONTRIBUTING.md) |
| Code of conduct | [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md) |
| Security | [`SECURITY.md`](./SECURITY.md) |
| Upstream history | [`docs/upstream-history/grok-app/`](./docs/upstream-history/grok-app/) |

## Contributing

Issues and PRs are welcome at <https://github.com/Po1nt9/omp-desktop>. See
[`CONTRIBUTING.md`](./CONTRIBUTING.md) for the development workflow.

## License

[MIT](./LICENSE) · Adapted from `RongleCat/grok-app` (MIT).
