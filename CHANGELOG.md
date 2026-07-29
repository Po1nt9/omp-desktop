# Changelog

All notable changes to OMP Desktop will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Repository established as OMP Desktop, adapted from RongleCat/grok-app (MIT)
  at commit `d2a2563f19bba46cb67496d3b4ac821a31bceaed`.
- OMP Runtime source pinned as submodule at `runtime/oh-my-pi`
  (commit `667111575ebba136dadfd6989379e7f67e0d40d9`).

### Removed

- Grok CLI install/probe/update/session modules and commands.
- Grok account, quota, SuperGrok, and direct xAI credential modules.
- `_x.ai/*` private protocol extensions and fixtures.
- Deprecated `remote-bridge/` Node package.
- SuperGrok-specific components and artwork.

### Known Limitations

- Agent execution returns `runtime_unavailable` until OMP integration lands.
- Provider authentication and runtime-owned configuration are unavailable.
- The model catalog is empty; no fallback model is hardcoded.
