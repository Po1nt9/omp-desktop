# Changelog

All notable changes to OMP Desktop will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added / 新增

- **Bundled omp Runtime:** the app now ships with the omp engine compiled from
  the pinned `runtime/oh-my-pi` submodule — works out of the box, no manual CLI
  install. Resolution order: manual override → in-app upgraded copy → bundled
  sidecar. Settings → About gains "Check OMP update"（检查 OMP 更新）to upgrade
  omp independently from upstream `can1357/oh-my-pi` releases.
- **内置 omp 引擎：** App 现在自带由 `runtime/oh-my-pi` submodule 编译的 omp——开箱即用。
  解析顺序：手动指定 → App 内升级副本 → 内置副本。设置 → 关于 新增「检查 OMP 更新」，
  可独立于 App 从上游 `can1357/oh-my-pi` 升级 omp。

## [0.3.1-nightly] - 2026-07-31

### Changed / 变更

- **In-app auto-update enabled:** builds are now signed with a minisign
  updater key, so Settings → About "check for update" runs the silent
  download → install → relaunch path (was: open GitHub release page).
  The rolling `omp-desktop-latest` release now ships `latest.json` +
  per-platform `.sig` archives. 应用内自动更新已启用（minisign 签名）。

### Added / 新增

- **Update channels (stable/beta/nightly):** the update feed is now isolated
  per channel. Channel identity is baked into each build from its version
  suffix; CI publishes per-channel rolling manifests
  (`omp-desktop-latest`/`latest.json`, `omp-desktop-beta`/`beta.json`,
  `omp-desktop-nightly`/`nightly.json`) and marks beta/nightly releases as
  GitHub prereleases. Settings → About shows the build's channel.
  更新渠道（stable/beta/nightly）隔离：渠道身份随构建固定，CI 按渠道发布
  滚动 manifest，关于页显示当前渠道。
- **Credential management guide:** new `docs/credential-management.md`
  documents where credentials live (OS secure store only — strict mode, no
  plaintext fallback), the `keychain:v1:` on-disk reference format, the
  six-step startup migration, and operator recipes (AC-12.3).
  新增凭据管理指南：凭据仅存系统安全存储（严格模式，无明文 fallback），
  记录 keychain:v1: 引用格式与六步启动迁移（AC-12.3）。
- **Guides batch (§12):** five new user-facing guides — provider setup,
  permission model, remote access risk, recovery boundary, and i18n — plus an
  OS-codesign warning table with signing costs in the update guide.
  (AC-12.2/12.4/12.5/12.6/12.7/12.8.)
  指南批量补齐（§12）：新增 Provider 配置、权限模型、远程访问风险、恢复边界、
  i18n 五篇指南，并在更新指南中补充 OS 签名警告表与签名成本。
- **Acceptance gap closure (AC-3.4/8.3/8.7/8.14):** the i18n gate now
  enforces `{var}` placeholder-set parity across locales; new remote_im
  tests cover allow_from whitelist accept/reject, credential rotation +
  revocation, and Discord attachment fetch against a mock HTTP server.
  验收差距补齐：i18n 门新增占位符一致性校验；remote_im 新增白名单、
  凭据轮换/吊销与 Discord 附件抓取测试。
- **macOS ARM64 acceptance run:** performance benchmarks (cold start 0.10s
  median, peak RSS 39 MB), release artifact verification (DMG + app.tar.gz
  SHA256 match), updater verifier green (ok=7 warn=1 fail=0), and brand
  inspection all pass on Apple Silicon. Flips AC-2.1/2.12/4.3/10.7/10.8
  to PASS; matrix now 55/12/90/1. Fixed bash variable expansion bug in
  `verify-updater-setup.sh` (fd9728a).
  macOS ARM64 真机验收：性能基准（冷启动 0.10s / RSS 39 MB）、产物
  SHA256 校验、更新验证器全绿、品牌检查通过；矩阵翻转至 55/12/90/1。
- **CI hardening (mock E2E + poison-free lock):** CI was red on all 3
  platforms — `cargo test` never built the `mock_acp_runtime` E2E binary,
  and the resulting panics poisoned the `std::sync::Mutex` around
  `OMP_DESKTOP_HOME`, cascading ~15 PoisonError failures. CI now builds the
  mock bin explicitly on every OS and `APP_HOME_ENV_LOCK` is a poison-free
  `parking_lot::Mutex`; store disk tests isolate their own temp HOME. All
  jobs green again (518/0/1 on Windows). Flips AC-2.11 (4-target packaging
  run, all 6 release jobs) and AC-10.6 (license files now confirmed inside
  the macOS bundle at Resources/_up_/) to PASS; matrix now 57/12/88/1.
  CI 加固：mock E2E 二进制显式构建 + 锁改为无中毒 parking_lot 实现；
  store 磁盘测试自隔离临时 HOME；3 平台 CI 全绿，4 目标打包发布全绿，
  授权文件确认打包进 macOS 应用；矩阵翻转至 57/12/88/1。
- **Release blocker fixed (`default-run`):** the 4-target packaging run
  bundled the 385KB `mock_acp_runtime` test harness as the app executable
  on all 3 platforms — Tauri picks the main binary by matching the package
  name or `package > default-run`, and with two auto-discovered bins
  neither matched. Added `default-run = "omp-desktop"`; rebuilt and verified
  macOS (15.5MB), Linux AppImage (21MB) and Windows NSIS (`omp-desktop.exe`
  GUI) all ship the real binary. Also completes AC-6.8: `strings` scan of
  all 3 rebuilt binaries finds no `sk-` API keys / private keys / embedded
  credentials. Matrix now 58/12/87/1.
  发布阻塞修复：4 目标打包曾把测试用的 mock 二进制打成主程序（三平台
  全坏）——为多 bin 项目补上 `default-run` 后重建并逐平台验证主程序
  正确；同时对 3 平台产物完成密钥模式扫描（AC-6.8，无任何明文密钥）；
  矩阵翻转至 58/12/87/1。
- **Install/upgrade/rollback acceptance on 3 platforms (AC-10.1/10.4/10.5):**
  the rebuilt artifacts were exercised end-to-end — macOS ARM64 in a real
  Aqua GUI session (DMG install → in-app update → version-pinned rollback,
  cold start 0.10s / RSS 39MB), Linux x64 via amd64 Docker + Xvfb
  (AppImage/deb/rpm install paths, 0.72s / 227MB — the RSS overshoot is an
  emulation artifact flagged for bare-metal re-check), and Linux ARM64 on a
  native aarch64 VM (binary install, 0.28s / 187MB; no packaged installer —
  release.yml has no aarch64-linux target). AC-10.7 SHA256 evidence refreshed
  for all 8 rebuilt artifacts. Matrix now 61/12/84/1; acceptance execution
  closed 2026-07-31 with the remaining BLOCKED items (Windows x64 / macOS
  Intel hardware, IM channel accounts, external auditor) waived by the owner.
  三平台安装/升级/回滚验收：macOS ARM64 真实 GUI 会话、Linux x64（amd64
  Docker + Xvfb）、Linux ARM64（原生 aarch64 虚拟机）逐一跑通；8 个重建
  产物 SHA256 证据刷新；矩阵翻转至 61/12/84/1。验收执行于 2026-07-31
  收尾，剩余 BLOCKED 项（Windows 真机 / Intel Mac / 渠道账号 / 外部审计）
  经负责人确认暂缓。

### Known Limitations / 已知限制

- Builds remain **not code-signed** by Apple / Windows (Plan 9, paid certs).
  The updater verifies archive integrity, but macOS Gatekeeper / Windows
  SmartScreen still warn on the updated binary. 更新包已签名校验，但 OS 代码签名仍未启用。

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
