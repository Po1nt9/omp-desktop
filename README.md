<p align="center">
  <img src="assets/logo.png" alt="OMP Desktop" width="128" height="128" />
</p>

<h1 align="center">OMP Desktop</h1>

<p align="center"><strong>适配 OMP Runtime 的开源 Tauri/React 桌面外壳</strong></p>

<p align="center">
  <a href="./README.md">中文</a> ·
  <a href="./README_EN.md">English</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License" /></a>
  <a href="https://github.com/Po1nt9/omp-desktop/stargazers"><img src="https://img.shields.io/github/stars/Po1nt9/omp-desktop?style=social" alt="GitHub stars" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="Platforms" />
  <img src="https://img.shields.io/badge/Tauri-2-orange" alt="Tauri 2" />
</p>

---

> [!NOTE]
> OMP Desktop 是一个开源的 Tauri/React 桌面外壳，正在适配 OMP Runtime。
> Plan 1 基线有意采用 fail-closed 策略：在版本化的 OMP 集成落地之前，Agent 执行、Provider 认证
> 以及运行时拥有的配置均不可用。

---

## 目录

1. [简介](#简介)
2. [状态](#状态)
3. [开发](#开发)
4. [平台](#平台)
5. [安装与首次运行](#安装与首次运行)
6. [来源](#来源)
7. [文档](#文档)
8. [贡献](#贡献)
9. [许可证](#许可证)

---

## 简介

OMP Desktop 是一个基于 Tauri 2 + React + TypeScript 的桌面外壳。它由上游
`RongleCat/grok-app` 项目（MIT）适配而来，作为 OMP Runtime 的宿主。

**技术栈：** Tauri 2 + Rust · React + TypeScript + Vite · Tailwind CSS

OMP Runtime 源码以 git submodule 形式固定在 `runtime/oh-my-pi`。

## 状态

Plan 1 基线有意采用 fail-closed 策略。以下能力返回 `runtime_unavailable`，尚未接入实时运行时：

- Agent 执行（不运行任何 prompt）
- Provider 认证（不提供登录）
- 运行时拥有的配置（无模型目录，无回退模型）

在版本化的 OMP 集成落地之前，请勿将以上能力作为可用功能对外宣传或写入文档。
冻结的总体设计文档位于
[`docs/superpowers/specs/2026-07-28-omp-desktop-design.md`](./docs/superpowers/specs/2026-07-28-omp-desktop-design.md)。

## 开发

环境要求：Node 22+、pnpm 9、Rust stable、Xcode CLT（macOS）。

```bash
pnpm install

pnpm dev          # 完整应用（Tauri + Vite）
pnpm dev:ui       # 仅前端

pnpm typecheck
pnpm test
cd src-tauri && cargo test

pnpm build
```

## 平台

OMP Desktop 面向三个操作系统：

| 平台 | 目标 |
|------|------|
| macOS | Apple Silicon + Intel |
| Windows | x64 |
| Linux | x64（AppImage / deb / rpm） |

## 安装与首次运行

### macOS（Homebrew — 推荐）

```bash
brew install Po1nt9/tap/omp-desktop
```

Homebrew 会自动清除 macOS 隔离标记——首次启动无安全弹窗。

<details>
<summary>手动安装（DMG）</summary>

1. 从 [Releases 页面](https://github.com/Po1nt9/omp-desktop/releases) 下载 `.dmg`，
   将 **OMP Desktop** 拖入「应用程序」。
2. 首次启动会被 Gatekeeper 拦截（「来自身份不明的开发者」）。绕过方式：
   - 右键点击 App → **打开** → 对话框中再次点 **打开**；**或**
   - 终端执行：`xattr -cr "/Applications/OMP Desktop.app"`
3. 绕过仅首次需要——之后启动恢复正常。

</details>

### Linux（一键脚本）

```bash
curl -fsSL https://raw.githubusercontent.com/Po1nt9/omp-desktop/main/scripts/install.sh | bash
```

脚本自动下载最新 AppImage 到 `~/.local/bin/omp-desktop` 并赋予执行权限。

<details>
<summary>手动安装（AppImage / deb / rpm）</summary>

从 [Releases 页面](https://github.com/Po1nt9/omp-desktop/releases) 下载：

- **AppImage**：`chmod +x OMP-Desktop_*.AppImage && ./OMP-Desktop_*.AppImage`
- **deb**：`sudo apt install ./OMP-Desktop_*.deb`（依赖 `libwebkit2gtk-4.1-0`、`libgtk-3-0`）
- **rpm**：`sudo dnf install OMP-Desktop_*.rpm`（或 `rpm -i`）

</details>

### Windows

1. 运行 `OMP-Desktop_*_x64-setup.exe` 安装程序（NSIS），或用绿色便携版 zip：解压后运行 `OMP-Desktop.exe`。
2. SmartScreen 可能提示「Windows 已保护你的电脑」→ **更多信息** → **仍要运行**。
3. 需要 WebView2 Runtime（Win10/11 通常已预装；缺失时安装程序会自动下载）。

> [!NOTE]
> 当前构建**尚未进行 OS 级代码签名**（Apple Developer ID / Windows Authenticode）。
> 应用内**自动更新**已签名（minisign），静默升级正常工作。
> OS 信任警告仅为外观问题——详见
> [签名要求](./docs/release/signing-requirements.md) 了解 Plan 9 剩余工作及免费替代方案（SignPath、Homebrew）。

### 首次运行：指定 OMP Runtime

OMP Desktop **不内置** Runtime——由你自行提供 OMP CLI，以便独立升级 Runtime。首次启动：

1. 安装 [OMP Runtime](https://github.com/can1357/oh-my-pi) CLI。
2. 在 OMP Desktop 中打开 **设置**。
3. 将 **手动 CLI 路径（Manual CLI path）** 指向 `omp`（或 `omp.exe`）二进制位置。
4. 此后 Agent 会话会启动你指定的 Runtime。

## 来源

OMP Desktop 基于 MIT 许可证的上游源码适配：

- **桌面外壳基线：** `RongleCat/grok-app`，提交 `d2a2563f19bba46cb67496d3b4ac821a31bceaed`
- **OMP Runtime submodule：** `runtime/oh-my-pi`，提交 `667111575ebba136dadfd6989379e7f67e0d40d9`

历史上游资料（计划、wiki、验收文档）保存在
[`docs/upstream-history/grok-app/`](./docs/upstream-history/grok-app/) 作为来源凭证。
这些文件不代表当前 OMP Desktop 产品。

## 文档

| 对象 | 入口 |
|------|------|
| 冻结总体设计 | [`docs/superpowers/specs/2026-07-28-omp-desktop-design.md`](./docs/superpowers/specs/2026-07-28-omp-desktop-design.md) |
| 品牌基线计划 | [`docs/superpowers/plans/2026-07-28-repository-brand-baseline.md`](./docs/superpowers/plans/2026-07-28-repository-brand-baseline.md) |
| 更新日志 | [`CHANGELOG.md`](./CHANGELOG.md) |
| 贡献指南 | [`CONTRIBUTING.md`](./CONTRIBUTING.md) |
| 行为准则 | [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md) |
| 安全披露 | [`SECURITY.md`](./SECURITY.md) |
| 凭据管理 | [`docs/credential-management.md`](./docs/credential-management.md) |
| Provider 配置 | [`docs/provider-setup.md`](./docs/provider-setup.md) |
| 权限模型 | [`docs/permission-model.md`](./docs/permission-model.md) |
| 远程访问风险 | [`docs/remote-access-risk.md`](./docs/remote-access-risk.md) |
| 恢复边界 | [`docs/recovery-boundary.md`](./docs/recovery-boundary.md) |
| 国际化 (i18n) | [`docs/i18n-guide.md`](./docs/i18n-guide.md) |
| 上游历史 | [`docs/upstream-history/grok-app/`](./docs/upstream-history/grok-app/) |

## 贡献

欢迎在 <https://github.com/Po1nt9/omp-desktop> 提交 Issue 与 PR。开发流程见
[`CONTRIBUTING.md`](./CONTRIBUTING.md)。

## 许可证

[MIT](./LICENSE) · 适配自 `RongleCat/grok-app`（MIT）。
