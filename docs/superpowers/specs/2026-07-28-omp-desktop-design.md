# OMP Desktop 1.0 Master Design / Roadmap

- 日期：2026-07-28
- 状态：**书面规格已冻结**
- 评审说明：产品方向已完成分章节口头确认；用户于 2026-07-28 审阅并批准本文。后续实质范围、baseline、安全门槛或正式渠道分级的变更必须通过书面变更记录重新确认。
- 产品名称：**OMP Desktop**
- 前端基线：[RongleCat/grok-app](https://github.com/RongleCat/grok-app)
- Agent Runtime：[can1357/oh-my-pi](https://github.com/can1357/oh-my-pi) 的团队 Fork
- 许可证：MIT

## 1. 文档定位与摘要

本文是 OMP Desktop 1.0 的 master design/roadmap，定义产品边界、跨仓库协议、安全模型、交付链和最终验收基线。它不是可由单一实施计划完成的任务清单；每个依赖计划都必须有独立规格、实现计划和验收记录。

OMP Desktop 是完全开源、跨平台的桌面 Agent 应用。产品沿用 Grok App 的 Tauri/React 工作台、交互方式和视觉语言，但移除 Grok App 专属的产品品牌、认证、CLI、账号、配额和私有协议耦合。Agent 后端由随应用捆绑的 OMP Runtime 提供。OMP Runtime 合法提供的 xAI Provider、Grok 模型名称、Provider endpoint 和原始 Provider 错误不属于待移除的 Grok App 耦合。

1.0 支持 macOS、Windows 和 Linux，覆盖 capability baseline 中承诺的会话、Provider/模型、工具、权限、MCP、Skills、斜杠命令、Todo、子 Agent、分支与回退、附件、用量、配置和诊断能力。远程消息能力被整理为可插拔平台适配器，并接入同一 OMP 会话和权限模型。界面正式支持 `en`、`zh-CN`、`zh-TW`。

## 2. 已确认的产品决策

以下决策均已完成口头确认，并已写入本文；书面冻结仍待用户确认。

1. 以 Grok App 为产品代码基线改造成 OMP 专用客户端，不保留 Grok 后端切换。
2. 保留现有信息架构、交互和视觉语言，不改造成 IDE 控制台或多 Agent 大屏。
3. 正式产品名为 **OMP Desktop**；所有用户可见品牌简称一律写作 **OMP**。
4. 命令、可执行文件、环境变量值、路径、协议方法和代码标识可使用小写，例如 `omp acp`、`_omp/desktop/v1/*`；技术标识小写不构成品牌违规。
5. Runtime 返回的用户可见品牌字段必须归一化：`agentInfo.title=Oh My Pi`、`Oh My Pi`、`oh-my-pi` 或小写品牌显示值在 UI、菜单、通知、日志摘要、诊断、远程消息、安装/更新界面统一显示为 **OMP** 或 **OMP Runtime**。任何用户可见表面（包括可展开技术详情）均不显示原始 Runtime 品牌值；原始值仅可保留在用户主动导出的脱敏协议诊断文件中，且该导出物明确标注为原始上游数据，不纳入产品品牌展示。扫描范围覆盖 React/Rust 源码、locale、Runtime metadata、auth/mode/config option、slash command 描述与错误、远程适配器、manifest、安装器、更新器、文档、fixture、snapshot、图片替代文本和发布元数据。
6. 全面替换 Grok App 自有的 Grok/xAI 产品名称、图标、应用标识、链接和专属文案，采用原创品牌资产。OMP Provider catalog 中合法的 xAI Provider、Grok 模型名称、Provider endpoint、认证方法和原始错误按其真实名称显示，不视为 OMP Desktop 品牌违规，也不得因此删减 OMP 的模型能力。
7. 1.0 支持 macOS、Windows 和 Linux；macOS 发布 Universal 构建。Windows ARM64 移出 1.0，后续须完成 Runtime/native/Tauri/安装器/更新器依赖链验证后再加入。
8. OMP Runtime 随应用捆绑，与桌面版本共同签名、测试、发布和更新。外部 OMP 仅用于高级诊断，1.0 不承诺正式运行支持。
9. 默认单个受监督 sidecar；1.0 不提供“每会话进程”产品 UI，只允许测试构建中的显式开关。
10. Desktop 与 CLI 的“共享”指向同一 **Active OMP Agent Directory + profile**，不等于固定 `~/.omp`。
11. 使用 Git submodule 固定团队 OMP Fork commit；补丁优先上游化，未合并补丁保持可审计。
12. OMP Desktop 建立独立 Git 仓库，同时选择性同步 Grok App，并定期同步 OMP 官方上游。
13. 产品采用 MIT License，保留全部上游与第三方声明。
14. 正式 locale 为 `en`、`zh-CN`、`zh-TW`，英语是源语言和最终回退语言。
15. 1.0 以 capability baseline 100% 覆盖为完成口径，不以“替换聊天后端”或含糊的“完整”描述替代清单。
16. 保留远程消息；远程审批遵循 OMP 权限结果，不强制额外 PIN，渠道不得扩权。聊天账号失陷风险必须明确披露。
17. 远程采用可插拔适配器。1.0 候选产品入口共 11 个，其中 10 个是固定正式项，微信个人号是条件正式项；飞书/Lark 共享实现但分别测试中国与国际 endpoint/区域配置，技术 adapter ID 因区域拆分可为 12 个。
18. 微信个人号属于非官方协议，存在封号、条款变更和持续可用性风险；若发布门槛无法持续满足则降为实验性，不构成 1.0 硬承诺。
19. 旧 Node `remote-bridge` 迁移后归档到历史分支，并从源码主线依赖和所有发布产物删除。
20. 更新渠道确定为 `stable`、`beta`、`nightly`，配置、缓存和签名策略相互隔离；1.0 默认 `stable`。
21. workspace 白名单仅用于 routing/cwd 控制，不是沙箱。远程安全执行以 OMP tool 层的路径与进程隔离 capability 为正式支持门槛。
22. Provider、模型目录、凭据生命周期、MCP discovery/config、queue、steer、journal recovery、thinking visibility 和 trace propagation 均通过 OMP Desktop Extension Protocol 明确定义，不假设标准 ACP 已提供。
23. 凭据统一由 OMP Fork 的 credential API/auth-broker 管理，Desktop 与 CLI 不各自定义系统安全存储命名或 OAuth refresh writer。
24. Runtime process model、并发、恢复和多会话假设必须由协议 capability 与契约测试共同验证；验证失败即禁用对应入口，不以版本号推测。

## 3. 十个依赖计划链

以下计划按顺序形成 1.0 依赖链。每个计划必须另有独立规格、实施步骤、测试矩阵、迁移/回滚方案与签字验收；后项不得绕过前项的协议或安全门禁。下一步只编写并执行**计划 1**的实施计划。

1. **仓库与品牌基线**：创建正式仓库、锁定双上游、引入 OMP Fork submodule、清除 Grok App 专属 Runtime 耦合和产品品牌依赖、建立带 Provider/model 允许项的品牌扫描与许可证基线。
2. **OMP Desktop Extension Protocol**：盘点现有 `_omp/*`，制定版本化 namespace、schema、capability、稳定 ID、错误和兼容/重放规则，并在 Fork 与 Host 落地。
3. **Supervisor、核心 ACP、事件 journal 与多会话**：验证单 sidecar、多会话、会话映射、崩溃恢复、journal 和 active turn 状态。
4. **配置、Provider、MCP、Skills 与安全凭据**：Active Directory discovery、配置真源、模型目录、auth-broker、系统安全存储和迁移。
5. **Todo、Subagent、branch、rewind、附件与诊断**：按协商 capability 建立产品入口和恢复边界。
6. **i18n**：三 locale 的静态校验、本地化 Runtime envelope、原生表面和品牌归一化。
7. **Remote Hub**：统一 adapter、身份/路由、权限竞态、审计、安全存储与 workspace 安全门禁。
8. **渠道按协议族分批**：长连接机器人、Webhook、联邦/开放协议、非官方协议分别交付并按适用 capability 验收。
9. **各 OS 打包与更新**：macOS Universal、Windows x64、Linux x64/ARM64，签名、SBOM、三渠道更新、迁移与回滚。
10. **1.0 集成验收**：执行 capability × OS × locale × runtime mode × channel 矩阵，关闭所有 release blocker。

计划 1 的完成门槛：正式 Git 仓库可复现；两个 upstream 与 submodule commit 可追溯；用户可见 OMP Desktop 品牌扫描零违规；Grok App 专属 CLI、认证、账号、配额和 `_x.ai/*` Runtime 调用为零，同时 OMP 合法的 xAI Provider/Grok 模型能力保持可发现且可用；MIT/NOTICE/SBOM 输入清单通过；后续九项各有独立规格入口但不提前实现。

## 4. 分阶段 Roadmap

- **阶段 0（计划 1–2）**：仓库、品牌、许可证与 Extension Protocol 基线。契约测试用于验证实现，不能替代协议规格。
- **阶段 1（计划 3–4）**：核心 ACP、多会话、恢复、配置、Provider、MCP、Skills 和凭据安全。
- **阶段 2（计划 5）**：其余 OMP 产品能力和诊断。
- **阶段 3（计划 6–8）**：i18n、Remote Hub 与渠道分批交付。
- **阶段 4（计划 9–10）**：跨平台打包、更新、回滚及 1.0 验收。

## 5. OMP Desktop Extension Protocol

### 5.1 独立交付物

阶段 0/计划 2 必须产出独立、版本化、可生成类型的协议规格。技术 namespace 采用 `_omp/desktop/v1/*`；这是对当前 `_omp/*` 的兼容演进设计，不宣称该 namespace 已存在。当前实现中的 `_omp/sessions/listAll`、`_omp/projects/list`、`_omp/usage` 及其他 `_omp/*` 方法/通知必须先盘点其请求、响应、错误、排序与生命周期，再决定映射、保留或废弃策略。

交付物必须包括：

- initialize capability descriptor（扩展版本、方法、通知、可选/必选 feature、schema digest、limits）。
- 每个方法与通知的 JSON Schema、示例、敏感字段标签和生成类型。
- session、turn、event、permission request、queue receipt、credential reference、project、model、MCP source 等稳定 ID 规则。
- 稳定错误码、`messageKey + args`、retryable/recoverable、技术详情边界。
- major/minor 兼容规则、未知字段处理、降级和废弃周期。
- cursor 分页、snapshot 边界、event journal replay、严格顺序/局部顺序、重复投递和 gap 处理。
- 连接重启后的 stable event ID、replay cursor、journal commit point 和 active turn status。

### 5.2 1.0 必选 capability baseline

1.0 捆绑 Runtime 必须协商成功：核心 ACP session/prompt/cancel/tool/permission/elicitation；扩展 queue/steer、Provider/model/catalog/credential lifecycle、MCP config/discovery、Todo、subagent、branch/checkpoint/rewind、usage/compaction、attachment、diagnostics、event replay/recovery、message localization envelope 和 thinking visibility。Host 与 Remote Hub 范围内的 trace correlation 是 Desktop 必选能力；贯穿 OMP Runtime、tool、subagent、MCP 和 Provider 的 trace propagation 是可选扩展，不阻挡 1.0 stable。除该明确标为可选的 trace propagation 外，缺少任一承诺 baseline 项时，该构建不能进入 1.0 stable。

前端不得按 OMP 版本猜能力。不支持的非 baseline 可选能力隐藏或禁用并显示稳定原因码。

### 5.3 Queue 与 steer

标准 ACP 当前不提供 queue/steer。现有“active turn 时 cancel，再串行发送新 prompt”不等于 steer。扩展必须定义：

- `queue` receipt ID、session/turn 绑定、FIFO 或显式 priority、accept/reject 状态、取消和 dequeue 通知。
- `steer` 目标 active turn ID、接收确认、应用顺序、过晚/冲突错误。
- sidecar/Desktop 重启后 receipt 查询、已提交/未提交边界及不自动重发规则。

未协商扩展时，UI 只能提供“取消当前回复，然后发送新 Prompt”，不得标记为 queue 或 steer。

### 5.4 Provider、模型与凭据 API

这些能力属于扩展，不是假设标准 ACP 支持。至少提供：

- Provider：稳定 ID、用户可见 message key、认证方法、配置 schema、状态、能力、区域、错误。
- Model catalog：稳定 ID、Provider ID、显示名、context window、输入/输出 modalities、tool/thinking 支持、reasoning levels、availability/deprecation、成本/用量元数据（若 Runtime 有真源）。
- Credential lifecycle：list metadata、begin auth、complete/cancel auth、replace、revoke、health、migration status；永不向 React 返回 secret。
- Session config：当前/默认模型、mode/reasoning option 的稳定 ID、schema、作用域和可变更时机。

## 6. 总体架构与进程门禁

```text
React Product Layer
  → typed Tauri commands/events
Tauri Host
  ├─ OMP Adapter（ACP + versioned extension）
  ├─ Runtime Supervisor / Session Coordinator / Event Projector
  ├─ Config Client / Credential Broker Client
  ├─ Remote Hub
  └─ Desktop Services
  → JSON-RPC stdio
Bundled OMP Runtime (`omp acp`)
  ├─ managed sessions / event journal
  ├─ config discovery and writers
  ├─ credential API/auth-broker
  └─ Active OMP Agent Directory
```

React 不直接访问 sidecar、Agent Directory、密钥库或平台 Token。OMP Adapter 是唯一 Runtime 协议入口。默认一个受监督 sidecar；多会话、并发隔离和 restart 行为必须经协议验证。测试开关可启动每会话进程，但 1.0 不提供该模式 UI，也不得用进程级配置覆盖伪装 per-session 隔离。

外部 OMP 可在安全模式中用于只读版本/路径/capability 诊断，不参与 1.0 正式会话、配置写入或自动更新。

## 7. Active OMP Agent Directory 与配置所有权

### 7.1 Discovery

不得硬编码 `~/.omp`。**Active OMP Agent Directory** 由 OMP 自身 discovery 按 `PI_CODING_AGENT_DIR`、所选 profile、平台默认目录及 project cwd 解析；Desktop 调用版本化 discovery/config API，并在诊断页显示最终目录、profile、project cwd、来源与只读/可写状态。Desktop 不自行复制优先级算法。

Session 文件是按 cwd 编码组织的嵌套 JSONL，不得假设固定 `agent/sessions/*.jsonl` 平铺路径。资源必须按 OMP 真源分别处理：

- settings：YAML；
- MCP：JSON；
- models：YAML/JSON（以 discovery 结果为准）；
- credentials metadata 与 usage：SQLite；
- Skills：目录；
- project settings：由 project cwd 与 OMP discovery 定位；
- sessions：cwd 编码的嵌套 JSONL。

“Desktop 与 CLI 共享”只表示二者解析到同一 active directory/profile。诊断页必须在二者不一致时明确提示。

### 7.2 写入规则

优先且默认只使用 OMP config API、专用 writer 与锁；不得构造一个“统一 YAML writer”处理异构资源。认证/OAuth、source precedence、未知字段、备份、原子写入、锁和热重载由 OMP 真源定义。协议能力缺失、schema 更高或 writer 不可用时，对应 UI 只读或禁用，不建立第二配置库。

OMP Desktop 自有窗口、locale、远程非敏感配置和 UI 偏好存入 Desktop 配置，不写入 Agent Directory。

## 8. 凭据架构与迁移

### 8.1 OMP Fork 组件

OMP Fork 拆分：

- `CredentialIndexStore`：仅管理 metadata、Provider/account 状态和 opaque reference。
- `SecretResolver` / `SecretStore`：统一 Rust helper，访问 macOS Keychain、Windows Credential Manager、Linux Secret Service。
- auth-broker：Desktop 与 CLI 共用的 credential API，且是 OAuth refresh 的唯一 writer。

真实 API key/OAuth envelope 存系统安全存储。`agent.db` 只存 metadata 和 `keychain:v1:<opaque-id>`。Desktop 与 CLI 均不得定义自己的 keychain service/account naming。系统安全存储不可用时必须阻止保存/刷新并给出可操作错误，不得静默明文 fallback。

远程平台凭据使用同一 helper，但采用与 Agent Provider 凭据隔离的 namespace、ACL 和 metadata 类型。

### 8.2 幂等迁移

Legacy SQLite secret 和现有 `channel-secrets.json` 采用双读单写迁移：

1. dry run 枚举、校验可解析性、目标 namespace 和冲突，不写入。
2. copy 到系统安全存储。
3. readback 并以常量时间比较验证 envelope/secret 等价。
4. 在 index/config 写入 `keychain:v1:<opaque-id>` reference，并事务提交。
5. 将 legacy 值标记 tombstone；双读仅用于尚未迁移项，所有新写只写系统存储。
6. 再次验证 reference 可解析后清理 legacy secret；`channel-secrets.json` 安全删除。

每一步记录非敏感 migration ID 和状态，可重复执行。失败时回滚未提交 reference，保留原值可读；已经验证提交的项不反向复制明文。清理失败保持 tombstone 并重试，绝不造成凭据丢失。

## 9. MCP、Skills 与斜杠命令

当前 OMP ACP 使用 `enableMCP:false`，且 `session/new` 需要 `mcpServers`。目标设计是 Host 调用 `_omp/desktop/v1/*` 版本化 config/discovery 方法取得 Runtime-resolved MCP list，再原样传入 `session/new`。MCP 的认证/OAuth、source precedence、项目覆盖、校验和热重载均由 OMP 真源定义。

若该扩展尚未完成，MCP UI 只能只读展示可诊断信息或整体禁用；不得由 Desktop 维护第二份 MCP 配置。Skills 和斜杠命令同样通过 discovery/capability 获取稳定 ID、用户可见 message key、参数 schema、作用域和错误。

## 10. 会话、事件与恢复

OMP session 是 Agent 上下文、分支和恢复真源。Desktop journal 只保存 UI 投影、草稿、窗口状态、App/OMP session 映射、远程绑定和审计索引。恢复优先 `session/load`，不得把 UI 历史作为 Prompt 重注入。

当前不能声称 `epoch + sequence` 可跨重启去重。1.0 扩展必须提供 stable event ID、replay cursor、journal commit point、active turn status 和 gap/replay 规则。获得这些 capability 后，Desktop 才可从 commit point 重建投影，并按 stable ID 幂等消费。

扩展尚未提供或恢复信息不完整时采用 conservative recovery：

- load durable session history；
- 将未完成 turn 标记为 `unknown/interrupted`；
- Desktop 不自动重放 Prompt、tool、edit 或 shell；
- 要求用户查看状态后显式开始新 turn；
- 副作用保证仅为“Desktop 不主动重放”；崩溃边界的 Runtime/tool 结果可能未知，不能承诺绝对不重复。

同一 OMP session 的 active-turn 数量和多会话隔离由 capability 声明，不由 Desktop 假定。

## 11. 权限与安全边界

OMP 当前的工具 tier、approval mode、per-tool policy 和 ACP gate 共同构成权限真源；本文不引入 Open Policy Agent（OPA）。当前不存在通用持久 policy API：计划 2 必须在 OMP Fork 中定义版本化 policy capability/API，只有 Runtime 协商该能力后，Desktop 才能写入持久策略。当前 `allow_always` / `reject_always` 仅视为 managed-session 内存作用域；UI 不得将其描述成跨重启永久规则。

### 11.1 逐路径决策表

| 路径 | 决策源与 gate | Desktop/Remote 行为 |
| --- | --- | --- |
| tool tier / approval mode / per-tool policy | OMP 当前配置与 session tools；未来持久写入仅走已协商的版本化 policy API | 只展示 Runtime 返回的结果和选项；未协商持久 capability 时不提供跨重启保存 |
| ACP bash | tool tier + policy + cwd/shell containment capability | 未批准不执行；展示命令、cwd、环境风险 |
| ACP edit | policy + canonical target path gate | 展示目标和 diff；越界拒绝 |
| ACP delete | 独立高风险 gate + canonical path | 不从 edit/move 批准继承；展示不可逆风险 |
| ACP move | source 与 destination 分别 canonicalize/gate | 任一越界即拒绝 |
| generic elicitation | Runtime schema/request ID | 仅返回 schema 允许值，不提升工具权限 |
| Provider safety | Provider/Runtime safety decision | Desktop 不绕过；错误保留稳定分类 |
| plan approval | active plan/turn ID gate | 只批准对应 plan revision，不泛化为工具授权 |
| subagent | parent policy + 显式继承/收窄规则 | 不得比 parent 扩权；MCP/workspace 限制必须继承 |

请求绑定 runtime instance、session、turn 和 request ID；超时、restart 或 turn 结束后失效。Desktop/Remote 的“第一条合法决定生效”只针对**同一个 pending request**，其他请求不受影响。单 sidecar 的进程级 override 对所有 session 生效，不能在 UI 中伪装 per-session 设置。

### 11.2 Workspace 与远程执行

workspace 白名单只控制 conversation routing 和 cwd 选择，不是文件或进程沙箱。安全隔离必须由 OMP tool 层提供：canonical path enforcement（含 symlink）、shell cwd/子进程 containment，以及 MCP/subagent 的约束继承。

1.0 将上述 Runtime capability 设为“远程执行工具”的正式支持门槛。缺失时，正式渠道只能聊天/只读能力；若测试构建允许危险自动权限模式，必须逐次醒目标注“无沙箱、可能访问授权目录外资源”，默认关闭且不得宣称安全隔离。

远程审批不额外要求 PIN；平台账号失陷可导致攻击者在 OMP policy 允许范围内操作，文档必须建议平台 MFA、严格用户白名单和最小权限。

## 12. i18n 与 Runtime 用户可见内容

正式 locale 为 `en`、`zh-CN`、`zh-TW`；英语为源语言和最终回退。旧 `zh` 迁移为 `zh-CN`，支持运行时切换。消息键按语义命名，CI 校验三语键、ICU 参数/类型、空值、硬编码、伪本地化与关键截图。

OMP Runtime 的用户可见字符串必须采用扩展 envelope：稳定 `messageKey` + typed `args`；若尚无稳定 key，则分类为“本地化外壳摘要 + 可查看脱敏原文”，不得把不可控原文嵌入本地化句子。覆盖：

- agent metadata（含 `agentInfo.title` 品牌归一化）；
- auth methods、mode/config options；
- slash command help/errors；
- permissions、tool titles/risk、plan approval；
- Provider/Runtime 错误；
- Todo/subagent/diagnostics；
- 远程 bot help/errors/approval。

模型输出、用户输入、项目文件和工具原始输出豁免翻译；原始 Provider 错误可在脱敏技术详情中查看。托盘、系统通知、更新器和前端未启动时对话框使用 Rust 精简 locale。产品名、模型名、命令、路径和代码标识默认不翻译。

## 13. Thinking visibility 与 trace

当前没有可依赖的逐事件 thinking visibility 声明。扩展必须为每类事件提供 visibility classification（例如 user-visible、desktop-only、remote-allowed、internal）及来源。未协商时，Desktop 可在明确的本地诊断表面按现有安全规则处理，但远程默认不转发 thought。

`traceId` 需要扩展在请求、事件、tool/subagent/MCP 边界传播。若 Runtime 未提供，只保证 Desktop Host 与 Remote Hub 自身范围内关联，不宣称贯穿 OMP 内部或 Provider。

## 14. Remote Hub 与渠道

远程适配器运行于 Tauri/Rust Host，统一契约包含能力声明、配置校验、生命周期、入站标准化、文本/附件发送、编辑或分段降级、交互回复、限流、重试和断线恢复。渠道只做协议转换，不直接管理 OMP session。

旧 Node bridge 迁移完成后归档至历史分支，并从主线依赖、构建脚本和发布产物删除。

### 14.1 候选入口与正式支持分级

1.0 候选产品入口共 11 个。以下 10 个是固定正式项，微信个人号是第 11 个条件正式项：

1. 飞书 / Lark（固定正式；共享实现；飞书中国 endpoint 与 Lark 国际 endpoint/区域配置分别测试，可形成两个 adapter ID）
2. 钉钉 Stream（固定正式）
3. Telegram（固定正式）
4. Discord（固定正式）
5. Slack Socket Mode（固定正式）
6. Matrix（固定正式）
7. LINE（固定正式）
8. QQ OneBot（固定正式）
9. QQ 官方机器人（固定正式）
10. 企业微信（固定正式）
11. 微信个人号（条件正式；仅在持续通过稳定性与条款风险门槛时计入正式支持，否则降为实验性）

实验性：WPS 协作、微博；微信个人号未达持续门槛时也归入此级。WPS 数字员工不宣称可用，直到凭据、入站和出站路由通过独立验收。

企业微信迁移要求：默认监听从 `0.0.0.0` 改为 loopback；拒绝未签名或未成功解密的请求；覆盖端口分配冲突、重启占用、反向代理来源和密钥轮换测试。

### 14.2 渠道验收按 capability 适用矩阵

每个平台声明 direct message、group、channel、thread、attachment、message edit、button、webhook/long-poll 等 capability。只测试适用项；不要求每个平台都支持群组或线程。所有正式渠道的共同必测项为凭据校验、入站/出站、身份白名单、审批过期与防重放、去重、限流/退避、撤销、日志脱敏；编辑、按钮、群组、频道、线程按声明能力测试。

平台 secret 使用统一 Rust SecretStore 的 remote namespace。所有渠道默认关闭；Webhook 默认 loopback，公网入口由用户显式配置反向代理或隧道。

## 15. 错误、诊断与可观测性

统一错误包含稳定 code/category/severity、recoverable/retryable、`messageKey`、args、可选脱敏详情和 recovery actions。trace 范围按第 13 节能力决定。

日志分 desktop、runtime、protocol、remote、updater；默认不记录 Prompt、回复、文件内容或完整工具输出，所有 key/token/cookie/header/疑似密钥脱敏。崩溃报告默认本地保存，外发前须用户预览并确认。

诊断页展示 Desktop/Runtime/扩展版本、sidecar 路径/签名、Active OMP Agent Directory/profile/discovery 来源、Provider 认证、MCP resolved sources、Skills、会话/进程、远程渠道、最近错误与 trace scope。一键自检只读，不发模型请求、不修改项目。

安全模式触发：sidecar 缺失/完整性失败、协议不兼容、连续崩溃、active directory 资源无法安全解析、凭据或配置迁移失败。安全模式允许查看投影、导出诊断、恢复备份和执行外部 OMP 只读诊断；禁止 Agent、工具和共享配置写入。

## 16. 打包、更新与上游治理

构建目标：

- macOS Universal（Apple Silicon + Intel）；
- Windows x64；
- Linux x64、Linux ARM64。

Windows ARM64 不在 1.0。发布包含签名、校验和、SBOM、`THIRD_PARTY_NOTICES`。Desktop 与捆绑 Runtime 是同一更新/回滚单元。`stable`、`beta`、`nightly` 三渠道隔离，更新流程必须校验签名/哈希、执行兼容预检、创建迁移检查点、安装后只读健康检查，并在失败时回滚或进入安全模式。

Grok App 仅选择性吸收 UI/无障碍、预览、Tauri 平台安全、正式渠道、更新/打包改进；禁止重新引入 Grok App 专属 CLI、xAI auth/account/quota、`_x.ai/*`、产品品牌资产和冲突状态机。此限制不适用于 OMP 自身合法的 xAI Provider、Grok 模型、Provider endpoint 或认证能力。OMP Fork 更新必须重跑协议、迁移和 capability baseline 验收，人工确认契约变化。

## 17. 许可证与品牌资产

Grok App 与 OMP 主许可证均为 MIT。OMP Desktop 使用 MIT，并保留：

- RongleCat 的 Grok App MIT 声明；
- Mario Zechner 和 Can Bölük 的 OMP/Pi MIT 声明；
- OMP 各目录 NOTICE；
- Silver 字体 CC BY 4.0 署名；
- Highlight.js BSD-3-Clause；
- 最终产物中全部 npm、Cargo、native 和资源依赖许可证。

安装包包含 `THIRD_PARTY_NOTICES`，应用内提供“关于 → 开源许可证”。不得沿用 Grok/xAI 名称、图标、bundle identifier 或造成官方关联混淆的资产。

## 18. 1.0 验收矩阵

### 18.1 维度

发布矩阵至少包含：

- **capability**：第 5.2 节 baseline 的每一项；
- **OS**：macOS Universal（分别在 Apple Silicon/Intel 运行）、Windows x64、Linux x64、Linux ARM64；
- **locale**：`en`、`zh-CN`、`zh-TW`；
- **runtime mode**：捆绑单 sidecar 正常、restart recovery、安全模式；每会话测试开关只进入非发布回归，不是产品模式；
- **channel**：10 个固定正式项、1 个条件正式项，以及飞书/Lark 区域拆分后可能形成的第 12 个 adapter ID；每个 adapter 还包含自身声明的 capability 适用项。

不要求对所有维度做无意义的全笛卡尔积。每个 baseline capability 必须在所有支持 OS 上通过；每个用户可见核心流程必须在三 locale 通过；recovery/security 项必须覆盖正常与故障 runtime mode；每个渠道按自身声明 capability 在至少一个受支持 OS 上跑真实 smoke，并在其余 OS 跑协议模拟/集成测试。

### 18.2 Pass 条件

1. capability baseline 清单实现、协议 schema、契约测试、产品入口和文档五者 **100% 对齐**；无未声明降级。
2. 所有 release-blocking 单元、协议、Tauri、前端、E2E、迁移、打包和更新测试通过，零未豁免 blocker。
3. 三 locale 的登记 message key 覆盖率 100%，ICU/变量校验零错误，关键流程截图通过；豁免仅限第 12 节原文类别。
4. 用户可见 OMP Desktop 产品品牌扫描在规定范围内零小写品牌、Grok App 专属品牌/Runtime 耦合遗留或未归一化 Runtime 品牌值。扫描允许项必须显式限定为 OMP catalog/runtime 真源中的 xAI Provider、Grok 模型名称、Provider endpoint、认证方法和脱敏原始 Provider 错误；这些允许项须保留真实名称并通过功能回归。
5. Active Directory/profile discovery 与 CLI 对照测试 100% 一致；Desktop 未绕过 OMP writer/lock。
6. 凭据迁移 dry run、成功、重复执行、系统安全存储不可用、readback 失败、清理失败和回滚测试全部通过；发布产物无明文 secret fallback。
7. crash recovery 证明 Desktop 不主动重放；未知边界正确标记 `unknown/interrupted`，文案不承诺绝对无重复。
8. 10 个固定正式远程项的共同必测项全部通过，适用项矩阵零失败。微信个人号通过持续门槛时作为第 11 个正式项发布；未通过时必须明确降为实验性且不计入固定正式支持承诺。固定正式项不得使用该条件降级规则规避验收。
9. workspace 只在 routing/cwd 层的表述一致；远程工具正式开启前，OMP canonical path、shell containment、MCP/subagent inheritance capability 全部通过。
10. macOS Universal、Windows x64、Linux x64/ARM64 安装、签名/校验、升级、回滚和 SBOM/许可证审计通过。
11. 外部 OMP 入口仅诊断；发布 UI 无每会话进程模式；三更新渠道配置正确隔离。
12. 文档覆盖安装、Provider、凭据、权限、远程风险、恢复边界、i18n、更新、许可证和贡献流程。

## 19. 工程约束

- React、Tauri Host 与 OMP Runtime 之间只使用版本化、类型化边界。
- React 不推测 OMP 内部状态，不读取 secret，不维护 Agent 配置/会话第二真源。
- Runtime API/writer/lock 优先于直接文件操作；缺 capability 时只读或禁用。
- 模块按可独立测试的功能边界拆分，不进行无关大规模重写。
- 所有用户可见 OMP 品牌文本必须为大写 **OMP**，第 2.4 节列出的技术标识除外。
- 协议规格与契约测试缺一不可；测试结果不能补写或替代规范。
- 本文冻结后，实质范围、baseline、安全门槛或正式渠道分级的变更必须走书面变更记录和重新验收。
