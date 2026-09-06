# 集成层重构：基线与验证记录

执行依据：本地 `drafts/refactor.md`。基线为 2026-09-06 的 `65ebdea`，目标是按功能所有权减少重复声明，保留数据布局、事务和运行时生命周期约定。本次按 Eric 指示跳过根决策检查。

## 进度

| 阶段            | 状态   | 交付                                              |
| --------------- | ------ | ------------------------------------------------- |
| 0：基线         | 已盘点 | 本文的登记点、command 与行为验证索引              |
| 1：contracts    | 待实施 | 显式响应模式、生成 client/DTO exports、确定性生成 |
| 2：Desktop      | 待实施 | 领域命令、Desktop binding、权限和 stream 接线     |
| 3：Backend 试点 | 待实施 | settings 窄 interface                             |
| 4：Backend 迁移 | 待实施 | 领域操作、生命周期协调和启动装配                  |
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
