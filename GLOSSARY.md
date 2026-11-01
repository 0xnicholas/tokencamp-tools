# tcamp

`tcamp` 是 `tokencamp-tools` 工具包的第一件工具：把编码 agent harness 配置到 tokencamp 并启动。本表只定义 `tcamp` 自己引入的概念；平台词汇按 [`tokencamp-pro/CONTEXT.md`](https://github.com/0xnicholas/tokencamp-pro/blob/main/CONTEXT.md) 原样复用。词头用英文，定义用中文，`_Avoid_` 列出中英禁用同义词。

## Language

### 工具与角色

**Toolkit**:
`tokencamp-tools` 仓库及其产物的集合，未来可以包含多件工具。
_Avoid_: project、package、suite

**Tool**:
工具包中一件可独立使用的产物；`tcamp` 是第一件。
_Avoid_: app、product、utility

**tcamp**:
工具包的第一件工具：单二进制 CLI，按 harness 分子命令，配置该 harness 并启动它。
_Avoid_: tokencamp（服务端二进制）、toolkit、wrapper

**Harness**:
`tcamp` 配置并启动的编码 agent CLI；v1 为 Claude Code、Pi、Kimi Code。
_Avoid_: agent、client、IDE、coding agent、tool

**Adapter**:
一个 harness 的配置契约与实现单元；一个 harness 恰好一个 adapter。
_Avoid_: integration、connector、plugin、provider

**Provider entry**:
harness 原生配置中代表一个 provider 的条目，例如 `providers.tokencamp`；只在 harness 配置语境使用。
_Avoid_: provider（单独指 adapter 或 connection 时）、adapter、connection

### 连接与凭据

**Base URL**:
tokencamp API 的根地址；规范化后不带 `/v1`，也是一个 connection 的身份。
_Avoid_: gateway、endpoint、server URL、host、API URL

**Connection**:
一个 Base URL 及其对应凭据；v1 一个 Base URL 一个 connection，没有命名 profile。
_Avoid_: profile、account、environment、instance

**Active connection**:
当前默认使用的 connection；`tcamp <harness>` 与 `status` 以它为准。
_Avoid_: active profile、current profile、default account

**Credential**:
本机保存的一条 API key 记录，归属于一个 connection。
_Avoid_: profile、account、secret、token

**API key**:
tokencamp 平台签发的密钥字符串；`tcamp` 只存储和使用，不创建、不吊销。
_Avoid_: token、secret key、key（单独使用有歧义时）、OAuth token

**Login**:
把已有 API key 存入本机并验证的动作；不创建 key，不走 OAuth。
_Avoid_: sign in、authenticate、connect

**Logout**:
删除本机凭据的动作；不是服务端吊销。
_Avoid_: revoke、sign out、disconnect

### 配置与所有权

**Config target**:
一个被 `tcamp` 写入的 harness 原生配置文件。
_Avoid_: config file（作为术语时）、settings、target file

**Desired state**:
`tcamp` 根据 active connection 与 model mapping 算出的、准备写入 config target 的配置。
_Avoid_: expected config、generated config、ideal state

**Managed entry**:
config target 中归 `tcamp` 所有的条目，可以是一个键、一张表或一个数组项。
_Avoid_: managed block、patch、snippet、owned key

**Overlay**:
Claude Code 专用的、整文件归 `tcamp` 所有的 settings 文件。
_Avoid_: settings file（泛指时）、drop-in、include

**Ownership manifest**:
与 harness 配置分开保存的所有权记录，记录每个 config target 上哪些 managed entry 归 `tcamp` 所有。
_Avoid_: registry、state file、ledger（与账本条目混义）

**Conflict**:
目标位置已有 `tcamp` 不拥有的条目或凭据来源，导致写入或启动被拒绝的状态。
_Avoid_: collision、clash、overwrite

**Adopt**:
把已存在的、非 `tcamp` 创建的条目纳入管理。
_Avoid_: take over、claim、import

**Drift**:
受管条目在 `tcamp` 写入后被用户手改的状态。
_Avoid_: divergence、mismatch、modification

**Shadowing**:
外部来源的凭据遮蔽 `tcamp` 启动时注入的 API key 的情况。
_Avoid_: override、conflict、masking

**Configure**:
把 desired state 写进 config target 的动作。
_Avoid_: setup、install、patch、write（单独使用）

**Launch**:
配置成功后启动 harness、并把进程控制权交给它的动作。
_Avoid_: run、start、setup、install

### 验证与运行

**Auth probe**:
免费、无副作用的 API key 校验请求。
_Avoid_: health check、ping、test call

**Verify**:
校验 API key 是否可用的动作；默认走 auth probe，显式深度校验才发最小真实请求。
_Avoid_: check、validate、test、ping

**Doctor**:
体检命令：检查本地配置与可选网络连通性，并报告问题。
_Avoid_: health check、diagnose、check

**Smoke**:
一次真实 harness 端到端调用，只用于验收/测试，不是 CLI 命令。
_Avoid_: e2e test、integration test、live test

### 模型

**Slot**:
harness 侧的模型位置或角色；Claude Code 用 haiku/sonnet/opus，Pi/Kimi 用默认/快速等。
_Avoid_: tier、variant、model slot、alias

**Model mapping**:
slot 到 Catalog model 的对应关系，由 `tcamp` 提供默认值，并允许用户覆盖。
_Avoid_: model selection、routing、preset

**Catalog model**:
tokencamp 模型目录中的一条 Model；与 slot 相对。
_Avoid_: model（单独指 slot 时）、upstream model、provider model

**Model reference**:
写进 harness 配置的模型标识，可以是 catalog id、`auto`、`@preset/...` 或家族别名。
_Avoid_: model name、model string、alias（单独使用）

### 共用词

以下词按 [`tokencamp-pro/CONTEXT.md`](https://github.com/0xnicholas/tokencamp-pro/blob/main/CONTEXT.md) 原样使用，不在此重定义：

**Model**、**Family Alias**、**Preset**、**Workspace**、**Budget**、**Provider**、**Deployment**、**Request**、**Generation**。

`Provider` 只指上游供应商；harness 原生配置里的对象用 **Provider entry**。
