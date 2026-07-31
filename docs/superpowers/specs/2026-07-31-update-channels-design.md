# OMP Desktop 更新渠道（stable/beta/nightly）隔离设计

**日期：** 2026-07-31
**状态：** 已确认（用户缺席先例：推荐默认值记录于此，可在 git 历史中评审）
**验收项：** AC-10.9（FAIL → PASS）· 关联 AC-10.8 / AC-2.12（live feed 验证，不在本设计范围）
**依据：** [Master Design §更新渠道](../specs/2026-07-28-omp-desktop-design.md)（"更新渠道确定为 `stable`、`beta`、`nightly`，配置、缓存和签名策略相互隔离；1.0 默认 `stable`"）

---

## 1. 背景与问题

当前实现（Explore 摸底 2026-07-31）：

- `updater.rs:75-81` 的 `channel` 字段只有 `silent` / `github_manual` 两个值——它们是**投递模式**（签名插件可用与否），不是发布渠道。
- 全仓库只有一个有效 feed：`omp-desktop-latest` 滚动 release 上的 `latest.json`，编译期经 `OMP_DESKTOP_UPDATER_ENDPOINT` 烘焙进 `tauri.release.conf.json`。
- 无 stable/beta/nightly 概念：无渠道身份、无分渠道 manifest、无分渠道 CI 规则（`release.yml:174` 甚至对 nightly tag 也强制 `prerelease: false`）。
- `app_update.rs` 手动回退路径查询 `/releases/latest` 且 `parse_semver` 丢弃预发布后缀——`0.3.1-nightly.20260731` 与 `0.3.1-nightly.20260730` 被判为相等，渠道内更新会漏报。

AC-10.9 因此判 FAIL："effectively single-channel"。

## 2. 外部方案调研结论（Librarian, 2026-07-31）

遵循"先调研再自建"原则，调研了 Tauri 2 生态的多渠道实践：

| 事实 | 来源 |
|---|---|
| `tauri-plugin-updater` 插件注册侧 `Builder` **无 `.endpoints()`**；endpoint 只能来自编译期配置（已在本地 crate 源码 2.10.1 核实） | tauri-plugin-updater/src/lib.rs |
| 运行时覆盖 endpoint 仅可经 `app.updater_builder().endpoints(...)`（Rust 侧 `UpdaterBuilder`）；JS `check()` 不接受 endpoints 参数 | docs.rs tauri-plugin-updater |
| `tauri-action` 无多渠道概念，`includeUpdaterJson: false` + 自建 manifest（本项目现状）正是多渠道的正确基础 | tauri-apps/tauri-action |
| semver 预发布序：`1.0.0-nightly.20260731 < 1.0.0`，`beta.2 > beta.1`，`nightly.20260731 > nightly.20260730`——**渠道内默认比较器天然正确，渠道间天然隔离** | semver 2.0 规范 |
| 桌面端成熟渠道模式（Chrome Canary/Beta/Stable、VS Code Insiders、Discord PTB）= **每渠道独立构建物**，渠道即安装物身份，而非运行时开关 | 业界实践 |

## 3. 关键决策（用户缺席 → 推荐默认值）

### D1：渠道模型 = 构建期烘焙，不做运行时切换（推荐）

**决策：** 渠道是构建物的固有身份，从**版本字符串**解析：预发布段含 `nightly` → Nightly，含 `beta` → Beta，否则 → Stable。CI 按 tag 后缀烘焙对应 feed endpoint；运行时从 `env!("CARGO_PKG_VERSION")` 解析同一身份。用户通过安装对应渠道的安装包选择渠道（Chrome/VS Code 模式）。

**否决的备选：**
- *运行时渠道切换（设置页下拉 + 重启生效）*：插件 `Builder` 无 `.endpoints()`（§2 已核实），JS `check()` 无法换 endpoint；要么重写 Rust 侧 check/download/install 全流程（1.0 前高风险），要么做半吊子的"重启后生效"且 download/install 仍走旧 endpoint（破坏隔离）。否决。
- *烘焙全部 3 个 endpoint 进配置，插件按序尝试*：插件会永远命中第一个可用 endpoint，等于没有隔离。否决。

**理由：** 隔离最彻底（nightly 构建物理上不可能看到 stable feed 之外的 feed 由构建物决定）；零新增运行时状态；CI 已有渠道信号（tag 后缀）；1.0 前回归风险最小。

### D2：feed 拓扑 = 3 个滚动 release + 3 个 manifest

| 渠道 | 滚动 release tag | manifest | 版本形态 |
|---|---|---|---|
| stable | `omp-desktop-latest`（沿用现有） | `latest.json` | `1.0.0` |
| beta | `omp-desktop-beta` | `beta.json` | `1.1.0-beta.1` |
| nightly | `omp-desktop-nightly` | `nightly.json` | `0.3.2-nightly` / `1.1.0-nightly.20260801` |

stable 沿用 `omp-desktop-latest` 保证现有文档/脚本/已烘焙 endpoint 的连续性。

### D3：签名 = 单一密钥对，全渠道共用（记录偏差）

Master design 要求"签名策略相互隔离"。密钥级隔离（每渠道独立密钥）带来真实的密钥管理成本，而安全相关属性——**一个渠道的 manifest 无法向另一渠道的用户投递更新**——已由 endpoint 隔离保证（D1/D2）。采用单一密钥对，偏差记录于此；若未来需要分团队签发 nightly，可用 `UpdaterBuilder::pubkey()` 运行时覆盖，无需架构变更。

### D4：每渠道独立应用数据目录（side-by-side 安装）→ 推迟到 1.0 后

Master design 的"配置、缓存隔离"在 Chrome 模式下意味着每渠道独立 app data 目录与 bundle identifier。但当前 productName/identifier 单一，**并行安装尚不可能**（安装路径相同，只能替换安装）；顺序替换安装下配置隔离无对象。随 side-by-side 支持一并推迟，post-1.0 立项。

### D5：过渡期双发布（保护已安装 nightly 用户）

现存已安装的 `v0.3.x-nightly` 构建烘焙的 endpoint 是 `omp-desktop-latest/latest.json`。nightly feed 迁至 `omp-desktop-nightly` 后，这些用户会被遗弃。**决策：** nightly tag 的 assemble 任务在过渡期内**同时**上传 `nightly.json` → `omp-desktop-nightly` 和 `latest.json` → `omp-desktop-latest`（CI 环境变量 `DUAL_PUBLISH_LEGACY_LATEST=1` 控制，release.yml 内注释标明 1.0 stable 首发后删除）。老构建经旧 feed 拿到最后一个双发布版本后，新构建的烘焙 endpoint 指向新 feed，迁移完成。

### D6：手动回退路径（app_update.rs）渠道感知 + 预发布感知比较

- 渠道身份同样从 `CARGO_PKG_VERSION` 解析，与签名路径一致。
- stable → 维持 `/releases/latest`（GitHub 语义已排除 prerelease；D7 的 CI 修复使其真正成立）。
- beta/nightly → `GET /releases?per_page=30`，取 tag 预发布段匹配渠道的**最新**一条。
- 版本比较换用 `semver` crate（tauri-plugin-updater 已在依赖树中引入，**编译产物零新增**），替换丢弃预发布段的自制 tuple 解析器。

### D7：CI 按 tag 推导渠道并修正 prerelease 标志

`release.yml` 从 tag 推导 `CHANNEL`（含 `-nightly` → nightly；含 `-beta` → beta；否则 stable）：
- `tauri-action` 的 `prerelease` 改为 `channel != 'stable'`——**顺带修复 stable 用户手动路径被 nightly release 污染的存量问题**（当前 nightly release 也标 `prerelease: false`，会占据 `/releases/latest`）。
- `build-release-config.mjs` 按渠道生成对应 endpoint。
- assemble 任务按渠道上传对应滚动 release/manifest（含 D5 双发布）。

### D8：前端只展示渠道身份，不提供切换器

About 页展示真实渠道徽标（stable/beta/nightly，来自 `updater_status` 新增 `release_channel` 字段）+ "渠道由安装包决定，切换渠道请下载对应安装包"的说明文案（3 语言）。不做下拉切换器（D1 已否决运行时切换）。

## 4. 架构与组件

```
tag push (v1.1.0-nightly.20260801)
        │  release.yml: CHANNEL=nightly（tag 后缀推导）
        ▼
┌─ build-release-config.mjs ── tauri.release.conf.json
│     endpoint = …/omp-desktop-nightly/nightly.json（按渠道）
│
├─ tauri-action ×4 平台 ── 签名归档 + .sig，prerelease=true
│
└─ assemble-updater-manifest.sh CHANNEL=nightly
        rolling=omp-desktop-nightly, manifest=nightly.json
        + 过渡期双发布 latest.json → omp-desktop-latest（D5）

运行时（已安装构建）：
  env!("CARGO_PKG_VERSION") = "1.1.0-nightly.20260801"
        │  update_channel::UpdateChannel::from_version → Nightly
        ▼
  签名路径：插件烘焙 endpoint（构建期 = 本渠道 feed）→ 渠道内 semver 比较
  手动路径：app_update.rs 按渠道选 release 源 + semver 比较（D6）
  状态上报：updater_status.release_channel = "nightly" → About 徽标
```

### 新增/修改组件

| 组件 | 变更 |
|---|---|
| `src-tauri/src/update_channel.rs`（新） | `UpdateChannel` 枚举 + `from_version` / `rolling_tag` / `manifest_name` / `feed_url(base)`；单元测试 |
| `src-tauri/src/updater.rs` | `UpdaterStatusDto` + `release_channel` 字段（从 `CARGO_PKG_VERSION` 解析） |
| `src-tauri/src/app_update.rs` | 渠道感知 release 选择（`select_release_for_channel`）；`is_remote_newer` 换 semver crate；保留 `parse_semver` 旧行为测试点迁移 |
| `src-tauri/Cargo.toml` | + `semver = "1"`（已在依赖树，零新增编译产物） |
| `scripts/update-channel-lib.mjs`（新） | JS 侧渠道推导单一事实源（tag/version → channel/rollingTag/manifest/endpoint/prerelease） |
| `scripts/build-release-config.mjs` | 用 lib 按渠道生成 endpoint |
| `scripts/assemble-updater-manifest.sh` | `CHANNEL` 入参 → rolling tag/manifest 名/标题推导；`DUAL_PUBLISH_LEGACY_LATEST` 双发布；`PRINT_DERIVED=1` 干跑模式（可测） |
| `.github/workflows/release.yml` | CHANNEL 推导 step；`prerelease` 按渠道；assemble 传 CHANNEL |
| `scripts/verify-updater-setup.sh` | `--fetch-latest` 支持指定渠道 manifest |
| `src/lib/api.ts` + `useUpdater.ts` + `SettingsPage.tsx` | `releaseChannel` 类型/展示；渠道说明文案 |
| `src/i18n/messages.ts` + `zh-tw.ts` | 渠道名称/说明文案 ×3 语言 |
| `docs/desktop-auto-update.md` | 多渠道架构重写 |

### 顺带修复（本工作包内）

版本号漂移：`Cargo.toml`/`Cargo.lock` `0.3.0-nightly` 与 `package.json`/`tauri.conf.json` `0.3.1-nightly` 不一致（release-tag.sh 未全程使用）。统一为 `0.3.1-nightly`（含 3 处 i18n 页脚）。渠道身份依赖版本字符串，此修复成为正确性问题而非清洁问题。

## 5. 数据流与错误处理

**渠道解析失败兜底：** `from_version` 对无法解析/不含渠道后缀的版本一律 → `Stable`（最保守 feed）。构建物版本恒为合法 semver（release-tag.sh 校验），该兜底仅防御本地/异常构建。

**手动路径网络失败：** 维持现状（API → HTML 重定向回退 → 报错文案），渠道选择逻辑失败（列表为空/无匹配 tag）→ `update_available: false` + 指向渠道 rolling release 页，不报错弹窗。

**endpoint 推导失败：** `feed_url` 要求 base 匹配 `…/releases/download/<rolling>/<manifest>` 形态；不匹配时全渠道回落 stable endpoint（编译期原值），绝不产生空 endpoint。

**CI 推导异常：** tag 不含后缀 → stable；`assemble-updater-manifest.sh` 对未知 `CHANNEL` 值直接 `exit 1`（fail-closed，防误发）。

**双发布开关：** 仅 nightly 渠道且 `DUAL_PUBLISH_LEGACY_LATEST=1` 时生效；stable/beta 永不双发（防污染 stable feed）。

## 6. 测试策略（TDD）

| 层 | 测试 |
|---|---|
| Rust `update_channel` | from_version 全形态（`1.0.0`/`1.1.0-beta.1`/`0.3.1-nightly`/`1.1.0-nightly.20260801`/非法串→stable）；rolling_tag/manifest_name/feed_url 映射；feed_url 异常 base 回落 |
| Rust `updater` | `updater_status` DTO 含正确 `release_channel` |
| Rust `app_update` | `is_remote_newer` 预发布序（nightly 日期序、beta 序号、stable>同版 nightly）；`select_release_for_channel`：stable 跳过 prerelease tag、beta 只取最新 `-beta`、nightly 只取最新 `-nightly`、空列表/无匹配 |
| JS `update-channel-lib` | tag/version → channel/endpoint/prerelease 全矩阵（node:test，沿用 `scripts/*.test.mjs` 模式） |
| bash `assemble-updater-manifest.sh` | `PRINT_DERIVED=1` 干跑断言三渠道推导值 + 非法 CHANNEL fail-closed（从 node:test 调起） |
| 前端 | `releaseChannel` 类型贯通；i18n 键 3 语言（`check:i18n` 门）；About 渠道徽标渲染（vitest，如该组件已有测试则扩展） |
| 门 | `cargo test --lib`、`pnpm test`、`pnpm typecheck`、`check:i18n`、`check:brand`、`check:provenance`、`check:legal` 全绿 |

**不可自动化部分（明确归属）：** 真实三渠道 feed 的端到端发布/拉取/签名验证属 AC-10.8/AC-2.12（BLOCKED，随跨平台验收跑真实 release）。本包交付的是隔离机制本身 + 配置审计证据。

## 7. AC-10.9 翻转标准

- Config audit：三渠道 endpoint/manifest/滚动 release 推导有自动化测试锁定（§6）✅
- Manual channel switch test：机制 = 安装对应渠道构建物；切换语义（渠道内 semver 序、渠道间隔离）由 §6 Rust/JS 测试断言 ✅
- live 三渠道并发发布验证 → 留在 AC-10.8/AC-2.12（BLOCKED），在矩阵条目中注明归属 ✅

翻转后 FAIL 清零（AC-12.3 凭据文档为下一工作包）。

## 8. 范围外（明确不做）

- 运行时渠道切换器（D1 否决）
- 每渠道独立签名密钥（D3 偏差）
- side-by-side 安装 / 每渠道 app data 隔离（D4 推迟）
- 真实发布流水线的端到端演练（AC-10.8/AC-2.12）
- 更新流程的兼容预检/迁移检查点/回滚安全模式（Master design §10 的其余要求，另行立项）

## 9. 自检

- 占位符扫描：无 TBD/TODO。
- 内部一致性：D1 与 §4 架构图、§6 测试矩阵一致；D5 双发布仅限 nightly 与 §5 fail-closed 规则一致。
- 范围：单一实现计划可承载（5 个 TDD 任务量级）。
- 歧义消除：feed_url base 异常→stable 回落（§5）；CHANNEL 非法→exit 1（§5）；双发布仅 nightly（§5）。
