# 集成层重构：基线与验证记录

执行依据：本地 `drafts/refactor.md`。基线为 2026-09-06 的 `65ebdea`，目标是按功能所有权减少重复声明，保留数据布局、事务和运行时生命周期约定。本次按 Eric 指示跳过根决策检查。

## 进度

| 阶段            | 状态   | 交付                                              |
| --------------- | ------ | ------------------------------------------------- |
| 0：基线         | 已盘点 | 本文的登记点、command 与行为验证索引              |
| 1：contracts    | 已完成 | 显式响应模式、生成 client/DTO exports、确定性生成 |
| 2：Desktop      | 已完成 | 领域 binding、生成接线和注册 guard                |
| 3：Backend 试点 | 已完成 | settings 窄 interface、独立存储测试               |
| 4：Backend 迁移 | 进行中 | 普通用例迁移中；聚合删除和运行控制随后迁移        |
| 5：前端归属     | 待实施 | feature 资源、查询和按需测试 transport            |
| 6：验收         | 待实施 | 增删演练、架构约束、完整 `task test`              |

## 人工登记点

以下按信息来源计数，不把生成文件、测试、文档和真实业务实现算作重复登记。演练场景分别为在已有领域新增 unary 和新增 stream。

| 人工来源                                       | Unary           | Stream          | 计划归属                          |
| ---------------------------------------------- | --------------- | --------------- | --------------------------------- |
| `ora-contracts` DTO 及所属模块导出             | 是              | 是              | 保留：契约所有者                  |
| `xtask/src/frontend/namespaces/` operation     | 是              | 是              | 保留，增加显式响应模式            |
| `xtask/src/export_contracts.rs` 类型与模块映射 | 新 DTO 时       | 新 DTO 时       | 阶段 1 删除，使用生成 DTO exports |
| `packages/contracts/src/index.ts` DTO exports  | 新 family 时    | 新 family 时    | 阶段 1 生成                       |
| `packages/contracts/src/client.ts` 成员转发    | 是              | 是              | 阶段 1 生成                       |
| `xtask/src/frontend.rs` stream 名称分支        | 否              | 是              | 阶段 1 删除                       |
| `tauri-transport.ts` command map               | 是              | 否              | 阶段 2 由 Desktop binding 生成    |
| `tauri-transport.ts` stream union 与判断       | 否              | 两处            | 阶段 2 从声明生成                 |
| `app_commands.rs` 注册                         | 是              | 新共享机制时    | 阶段 2 由 Desktop binding 生成    |
| `permissions/main-commands.toml`               | 是              | 新共享机制时    | 阶段 2 从显式授权生成             |
| `commands.rs` 领域 stream 分发                 | 否              | 是              | 阶段 2 生成接线，领域提供启动实现 |
| `Backend` 根 façade                            | 经过 Backend 时 | 经过 Backend 时 | 阶段 3–4 由窄领域 interface 取代  |
| `test/mock-client.ts` 完整 client              | 是              | 是              | 阶段 5 改为按需测试 transport     |

最终演练应证明：人工仅维护契约、逻辑 operation、Desktop binding 和业务实现；增加领域允许增加少量显式组合项。生成物数量不作为失败指标。

## Desktop command 与授权基线

- 110 个 SDK operation：105 个 unary，5 个 stream。
- Stream：`loadSession`、`promptSession`、`watchAppEvents`、`watchWorkspace`、`watchProject`。
- 130 个已注册 command：105 个 SDK unary binding，加 25 个共享 transport 或 Desktop 原生命令。
- 共享 transport：`stream_contract`、`cancel_contract_stream`。
- workspace 原生命令：`get_worktree_root`、`set_worktree_root`、`resolve_task_cwd`、`resolve_workspace_cwd`。
- 本地交互：`open_location`、`open_external_url`、`write_workflow_export`、`download_today_log`。
- surface：`surface_capabilities`、`surface_list`、`surface_open`、`surface_close`、`surface_set_bounds`、`surface_set_visible`、`surface_popout`、`surface_dock`、`surface_reload`、`plugin_webview_invoke`、`surface_resolve_download`、`surface_discard_download`。
- 更新：`get_desktop_update_status`、`install_desktop_update`、`check_desktop_update`。

`default.json` 将 `allow-main-commands` 赋予 `main` Webview，并包含独立的 Tauri core/dialog 权限。`plugin-webviews.json` 仅向 `plugin-webview:*` 赋予 `allow-plugin-webview-invoke`，只允许 `plugin_webview_invoke`。重构后应逐项保持这两个 capability 的授权集合，不能从 handler 存在推断授权。

## 行为验证索引

此表记录已读取的代表性测试，不把源码存在等同于测试已通过，也不把局部证据等同于完整链路覆盖。迁移时补齐缺口并更新路径。

| 验证义务                          | 基线直接证据                                                                                                                            | 覆盖判断 / 后续义务                                                  |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| client 保留 DTO、返回值和调用选项 | `packages/contracts/tests/client.test.ts`：完整请求、unary options、stream 请求、所有 endpoint 成员                                     | 部分：补 stream AbortSignal 与所有 operation 的实际路由              |
| 公共错误与 requestId              | `apps/desktop/web/tauri-transport.test.ts`：`normalizes structured command errors`                                                      | 部分：错误解码器对 malformed/unknown 的完整验证需继续核对            |
| lazy stream、事件顺序、单次消费   | 同文件：`starts channel streams lazily and forwards ordered data until end`                                                             | 已有该路径的直接测试；取消竞争单独补证据                             |
| stream 队列限制                   | 同文件：`fails a channel stream when its bounded consumer queue overflows`                                                              | 已有该路径的直接测试                                                 |
| 取消和断开释放注册、只完成一次    | `stream_forwarding.rs`：`cancellation_completes_the_request_once_and_releases_the_registration`、两项 channel disconnect 测试           | 部分：预取消、创建中取消、重复 id、shutdown 需在阶段 2 逐项核对/补齐 |
| request 生命周期共享完成权        | `request_lifecycle.rs`：`cloned_lifecycles_share_the_id_and_one_completion_claim`、完成后 drop                                          | 已有直接测试；跨 Desktop 入口仍需集成检查                            |
| 共享 worktree 租约阻止独占删除    | `git_cleanup/keyed_locks.rs`：`exclusive_waits_for_shared_holders`                                                                      | 部分：底层锁证据不能替代 task/project 删除链路                       |
| cleanup 恢复和所有权检查          | `git_cleanup/tests.rs`：`expired_lease_is_reclaimed_and_cleaned`、`live_lease_is_not_reclaimed`、`ownership_loss_parks_without_removal` | 已有局部直接测试；领域迁移时保留                                     |
| 更换 worktree root 后删除旧 task  | `bootstrap.rs`：`deletes_existing_task_after_worktree_root_changes`                                                                     | 已有该升级场景的直接测试                                             |
| 存储重开和启动装配                | `bootstrap.rs`：`opens_storage_and_serves_shared_crud_apis`；Desktop `lib.rs` 重开测试                                                  | 部分：settings 独立持久化、失败及重新打开在阶段 3 验证               |
| workflow 并发完成、取消和恢复     | 尚未完成验证义务到代表性测试的映射                                                                                                      | 未验证：阶段 4 迁移前补齐，不能以纯 helper 测试代替                  |

## 检查记录

后续每阶段记录实际运行结果。跨范围实现阶段最终运行完整 `task test`，Desktop 变化另及早运行 `task test:tauri`。最终包含增删演练后的证据与剩余手写接点说明。

### 阶段 1（2026-09-06）

- `cargo test -p xtask`：17 项通过，包括未知 stream 增删、重复声明拒绝、生成清单漂移和手写文件冲突保护。
- `cargo clippy -p xtask --all-targets -- -D warnings`：通过。
- contracts lint 与 9 项测试通过；新增遍历全部 110 个 operation 的实际调用验证，覆盖 DTO、响应模式、返回值及选项。stream AbortSignal 传递有单独直接断言。
- `task export-contracts` 后执行 `task check:contracts`：通过。检查在临时目录生成，能够识别未跟踪的新增/失效产物，不修复工作树。
- `task test`：完整通过，包括 frontend、Rust workspace、Tauri 与 4 项 Desktop E2E。测试入口现在检查生成产物，而不是先覆盖产物再验证。
- 已删除手写 client 成员表、类型名到文件名映射和中央 stream 模式推断。`client-runtime.ts` 只保留映射类型及通用调用逻辑；DTO 与静态 factory 由生成器输出。

### 阶段 2a：命令归属（2026-09-06）

- `commands.rs` 从 1576 行降至 124 行，仅保留请求执行、命令宏和领域组合；具体实现位于 `commands/`。
- task/settings 原有入口已迁入同一领域目录；删除两套重复请求执行逻辑，文件读取也使用可注入 context 的通用阻塞执行方法。
- workspace listing/diff/location、Effect status 和 workflow export 按职责归属；旧路径引用同步更新。command 名称、DTO 与 capability 集合未改变。
- `task test:tauri` 通过：包含 Clippy、53 项 Desktop 测试；最终阶段 2 仍需验证生成 binding、stream 竞争及全量测试。

### 阶段 2b：声明、分派与取消所有权（2026-09-06）

- Desktop 的 `bindings/` 按领域拥有 handler、响应接线及显式授权；xtask 关联逻辑 catalog，生成 transport map、stream 分类、类型化请求分派、Rust registry 与权限清单。公共 manifest 不含 Tauri 路径或授权。
- 与 `65ebdea` 逐项比对：主 Webview 的 130 项实际授权、plugin Webview 的唯一授权均不变；两个 capability JSON 逐字不变。生成过程去除了原权限列表中重复的一项 `rename_workflow_run`，不改变授权集合。
- 注册 guard 在领域创建前取得 id；取消只发信号，释放资源后才允许复用 id，避免旧任务清掉新注册。退出时统一取消并拒绝新注册；创建中取消等待创建安全结束后释放资源。
- 新增 catalog 完整性、错误模式、重复 handler、typed stream 增删和实际 plugin grant 校验；Rust 覆盖预取消、创建中取消、创建失败的 requestId、重复 id、shutdown。Frontend 覆盖创建中发出取消、终止错误及 requestId、未知 operation、预取消不启动 IPC；保留单次消费、顺序、end 与溢出验证。
- 验证：xtask Clippy 和 20 项测试通过；Tauri Clippy 和 58 项测试通过；Desktop frontend 41 项测试通过。最终 `task test` 全量通过，包含 4 项 E2E。

### 阶段 3：settings 试点（2026-09-06）

- 原私有 `UserConfigApi` 成为明确导出的 `Settings`，仅开放设置用例。构造、SQLite、原始配置键和 worktree root 持久化保持内部可见；没有新增 trait 或把 repository 暴露给 Desktop。
- 删除根 `Backend` 的 9 个设置入口，以一个 `settings()` 入口替代；proxy probe 和日志存储 capability 由 settings 拥有。请求执行和 updater 不再持有整个 Backend，runtime logging 仍只得到受限的 preferred-level store。
- 去掉启动中同一配置 module 的重复构造；通用的 Backend repository 执行机制独立于 bootstrap，避免领域 module 反向依赖启动装配。未改数据库或目录布局。
- 将完整 runtime CRUD 测试里的设置断言迁到 `settings/tests.rs`，补上重新打开、真实 SQLite 写失败及读失败测试；3 项通过。原 Desktop 重开/启动覆盖保留，且已迁移到窄 interface。
- 试点净收益：不是多套一层转发，而是移除根转发和不必要的 runtime 所有权，设置测试只需 SQLite。可以据此扩大阶段 4。
- 额外尝试的 `cargo clippy -p ora-backend --all-targets -- -D warnings` 被现有测试大量使用 `unwrap`/`expect` 阻挡（仓库标准 lint 不包含这些测试目标）；不顺带改写无关测试，最终按 `task test` 的标准门禁验收。
- 最终 `task test` 全量通过：含 208 项 Backend 测试、58 项 Tauri 测试和 4 项 E2E；标准 Rust/Frontend lint 均通过。

### 阶段 4a：普通领域用例（2026-09-06）

- 30 个 agent 定义、Skill 和 workflow 定义操作不再占用根 `Backend` 方法，调用者使用 `agents()` / `skills()` / `workflows()`。只导出所属用例，构造与字段仍隐藏，错误投影收进领域 module。
- Desktop 命令捕获对应领域 handle；surface 的自动下载和用户确认导入都使用 Skill interface。原导入流程、事务和 Effect 唤醒归属不变，未改动生成 binding 或任何 operation/DTO。
- 新增仅用 SQLite 的 agent CRUD/错误投影、workflow definition/draft 重开测试；既有 Skill 存储互斥和真实导入/Effect E2E 继续承担原验证义务。
- 此处只是阶段 4 的第一组。旧 command 宏分支暂时只服务尚未迁移的其他领域，最终阶段 4 收拢时删除；不把拆出这些普通用例等同于完成生命周期协调迁移。
- 验证：Backend 209 项通过、1 项原有测试保持 ignored；`task lint:crates`、`task test:tauri`（58 项）、`task test:e2e`（4 项）和 `task check:contracts` 通过。阶段 4 完成时再运行全量检查。

### 阶段 4b：project/task 聚合用例（2026-09-06）

- 再移除 12 个根 operation 转发，以 `projects()` / `tasks()` 交付完整用例。SQLite 级联、活跃后代检查、历史清理、阻塞线程派发和提交后的 Git cleanup 唤醒由所属领域拥有，Desktop 不再了解这些步骤。
- 新的 crate-private `TaskSetup` 显式注入原有 provisioning gates 与 cleanup handle，未新建锁或改变共享关系；根 Backend 不再保留只为转发而持有的 cleanup handle，删除无调用者的公开 repository pool 入口。
- project/task 两个 Desktop 删除命令原先绕过通用 lifecycle，现使用相同的 async executor，成功及失败都有相关联的完成记录。
- 旧 worktree-root 变更后删除测试移至 `task/lifecycle_tests.rs`，两项生命周期测试均在 scoped TRACE 下执行。新增真实 SQLite/Git 验证：Running session 同时阻止 task/project 删除且保留完整对象；停止后 task 删除连带隐藏 session，已有 worktree use lease 保证物理目录不被提前移除。
- workspace 查询、Git 操作及配置归属尚待下一组迁移；本组不改变磁盘布局和 Git cleanup 的恢复/退出语义。
- 验证：Backend 210 项通过、1 项原有测试 ignored；标准 Rust lint、58 项 Tauri、4 项 E2E 与生成漂移检查通过。

### 阶段 4c：workspace（2026-09-06）

- `WorkspaceDiffApi` 扩展为所属 module 的 `WorkspaceApi`，整合查询、live cwd、worktree-root 配置和 Git review；移除 10 个根入口，其中无调用者的 persisted-root 原始行查询不再公开。
- Desktop workspace/files 命令只注入 cloneable workspace handle；泛用文件浏览、搜索和 watcher 创建仍在 Desktop/`ora-fs`，没有迁移到 Backend。
- handle clone 共享原来的 root `RwLock`、SQLite pool 和 cleanup use leases，不新建锁/worker。`workspace.rs` 生产部分约 330 行；既有 main-checkout、task worktree 和 diff/commit/push 测试随 module 移动。
- 验证：Backend 210 项通过、1 项原有测试 ignored；标准 Rust lint、58 项 Tauri、4 项 E2E 和生成漂移检查通过。

### 阶段 4d：插件操作与 runtime 协调（2026-09-06）

- 20 个根入口（含 gateway 与下载进度变体）迁至 `Plugins`。安装、导入、更新、删除和扫描后的 agent-set 同步仍由同一 `AgentRuntimeManager` 完成，调用者只执行一个领域用例。
- 新协调代码放在约 210 行的 `plugin/operations.rs`。原大型 `PluginApi` 保持 crate-private，继续作为 runtime/Effect/configuration/gateway 共用的 host implementation；没有复制 lifecycle、锁、registry 或 generation 规则。
- 安装冲突与 README 测试迁至该 module，并通过公开 `Plugins` interface 执行；Tavily/configuration 场景从 bootstrap 随职责移动。需要外部发布产物的一项测试仍 ignored，其他依赖 `.tmp` 产物或 `ORA_E2E_PLUGIN_DATA` 的场景保留原条件，不将缺少 fixture 时的早退当成完整集成证据。
- 验证：Backend 210 项通过、1 项 ignored；标准 Rust lint、58 项 Tauri、4 项 E2E 与生成漂移检查通过。本次未配置 live plugin-home fixture。

### 阶段 4e：workflow-run 生命周期（2026-09-06）

- 12 个根 operation 迁到 `WorkflowRuns`，连同手工完成的 claim/prepare/revalidate/commit 和取消后的 session 清理一起迁移。Desktop 的 cancel/complete 也进入统一 async lifecycle，成功和失败均有 requestId 关联的完成记录。
- `WorkflowRunSetup` 注入原来的 engine、runtime、run locks 和完成中集合；自动回调、手工操作与尚待迁移的 session prompt 仍共享同一实例，没有新建 supervisor 或锁。
- boot sweep 和 baseline pruning 迁到 `workflow/run/recovery.rs`，启动调用顺序与 best-effort 语义不变。新的 operations 生产文件小于 500 行。
- 6 项公开 interface 测试使用生产 engine/runtime 与真实 SQLite：并发完成恰好一次、取消/完成竞争、history 读取失败后释放 claim 重试、session 清理失败不撤销取消、重开保留 awaiting node 并清理 orphan baseline、重开失败化中断 turn 并恢复 stalled run。没有外部 agent 进程的场景不被当作活跃 actor 取消证据，后续 session 迁移仍须覆盖。
- 测试共用已有真实数据库 fixture；内部 turn-policy 测试也改为 scoped TRACE 执行，避免共享日志 callsite 污染。
- 验证：Backend 216 项通过、1 项 ignored；标准 Rust lint、58 项 Tauri、4 项 E2E 与生成漂移检查通过。

### 阶段 4f：session 与 workflow prompt 协调（2026-09-06）

- 13 个 session operation 和 app-event 订阅转发从根 Backend 移除。`Sessions` 直接拥有原查询/改名 handlers，连同 unpublished-session 过滤、title actor adoption 和提交后的通知一起封装，不再叠加私有 `SessionApi` 转发。
- workflow-run 创建唯一的完成中集合；通过 crate-private `WorkflowSessionTurns` 向 Sessions 提供 prompt 准入与失败/stream-drop 清理能力。根 Backend 不再持有 run locks 或完成中集合，Desktop 只捕获 session handle。
- 缺少 agent 时的历史回放测试改为通过公开 Sessions 执行；新增真实 SQLite 改名成功/失败通知测试，以及 prompt 启动失败在返回前恢复 awaiting node 的测试。
- E2E fake ACP 增加显式 held prompt：收到首帧后一直等待真实 ACP cancel，不靠延时制造竞争。新增真实 actor/进程链路验证 stream drop 后 session 可复用、活跃 session 删除后记录与历史清理、workflow 取消后 actor 停止，以及人类 follow-up stream drop 后恢复 awaiting 并可手工完成。
- 验证：Backend 219 项通过、1 项 ignored；标准 Rust lint、58 项 Tauri、7 项 E2E 与生成漂移检查通过。runtime status、Effect status 和无状态 identity 是阶段 4 余下收口项。
