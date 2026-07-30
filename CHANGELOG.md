# Changelog

All notable changes to OMP Desktop will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0-nightly] - 2026-07-30

First end-to-end release of OMP Desktop with the OMP Runtime bridge wired, all
14 IM channel adapters connected, and the packaging/release pipeline exercised
on GitHub Actions. **Unsigned** — macOS Gatekeeper and Windows SmartScreen
warnings are expected; see the README install guide for bypass steps.

首个端到端发布的 OMP Desktop：OMP Runtime 桥接已打通，14 个 IM 渠道适配器已连接，
打包/发布流水线在 GitHub Actions 上完整验证。**未签名**——macOS Gatekeeper 与
Windows SmartScreen 警告属预期，绕过步骤见 README 安装指南。

### Added / 新增

- **Remote IM Runtime Bridge (Plan 7):** the `remote_im` engine's fail-closed
  gates are replaced with real OMP Runtime calls — per-`work_dir` AcpClient pool,
  drain barrier, and 3-layer concurrency locks. Inbound IM messages drive actual
  agent turns. 远程 IM 引擎接通真实 Runtime（按工作目录的 AcpClient 池 + 排空屏障
  + 三层并发锁）。
- **Inbound media (P2):** images sent over Feishu / Telegram / Discord are
  downloaded and passed to the agent as base64 ACP image blocks. 飞书/Telegram/Discord
  入站图片转 base64 传给 Agent。
- **Dedup + rate limiting (P1):** SQLite-backed message dedup (7-day TTL) and
  per-channel/per-scope rate limiting at the engine entry. SQLite 去重（7 天 TTL）+
  分渠道/分作用域限流。
- **Session portability (P3):** EventJournal persists to disk after each commit;
  sessions export/import as portable bundles. 事件日志落盘 + 会话导出/导入。
- **14 channel adapters:** feishu, telegram, discord, slack, dingtalk, wecom,
  weixin, qq, qqbot, matrix, line, weibo, wpc_xiezuo, generic.
- **Install & first-run guide** (bilingual EN/中文) in `README.md` / `README_ZH.md`,
  including the Gatekeeper/SmartScreen bypass steps and pointing the app at the
  user-supplied OMP Runtime CLI.
- **Auto-update documentation** (`docs/desktop-auto-update.md`) and
  **signing requirements** (`docs/release/signing-requirements.md`).

### Changed / 变更

- **Packaging pipeline verified:** the release workflow builds for all four
  targets (macOS ARM + x64, Windows x64, Linux x64) and publishes installers,
  a Windows portable zip, SHA256SUMS, and gracefully degrades the updater when
  signing secrets are absent. 发布流水线四目标构建验证通过。
- **OMP Runtime is user-supplied, not bundled:** the app points at a
  user-installed `omp` CLI (Settings → Manual CLI path) so the Runtime can be
  upgraded independently. App 不内置 Runtime，由用户在设置中指定 CLI 路径。

### Known Limitations / 已知限制

- Builds are **not code-signed** — the macOS/Windows signing blocker (Plan 9)
  requires purchasing certificates. 构建未签名，签名证书待购买。
- No in-app auto-update yet (updater secrets not configured). 暂无应用内自动更新。

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
