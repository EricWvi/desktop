# Controller–Node Protocol

[English](controller-node-protocol.md) | 中文

本文是 `feat(node-protocol): define worktree execution contracts` PR 的实现计划，作为
PR 中首个 docs commit 提交。该 PR 新增 `ora-node-protocol` crate，定义本机 Worktree
闭环所需的会话、执行与 framing 契约。本 PR 不实现 Transport，不启动 Node 进程，
不执行 Git 操作，也不加入 Controller 持久化。PR 末尾的 docs commit 再根据最终实现
移除 plan 表述并同步最终接口。

首个闭环限定为 Desktop、一个本机 Node，以及目标 Node 上已经存在 Main Workspace 的 Workspace。
Cloud、SSH 和其他 Transport 在本机切片验证后复用同一协议。

## 模块 seam

`ora-node-protocol` 是 Controller 与 Node 之间共享的协议 seam。两侧都依赖它公开的消息和 framing
接口；Transport adapter 也依赖同一接口，但不改变消息语义。

这个 crate 负责：

- 协议身份和版本值；
- 会话握手、心跳、执行状态查询和结果确认消息；
- 消息 envelope 及 Worktree 命令／结果类型；
- 这些命令和结果使用的协议侧 Node domain model；
- 定长分隔的 frame 编码和解码；
- 协议层校验错误；
- 验证分片 I/O、畸形 frame 和消息往返的测试。

这个 crate 不负责：

- Unix socket、Windows Named Pipe、stdio 进程管理、SSH 或 TLS；
- Node 启动、注册持久化、事件重放存储或去重状态；
- Git、文件系统、worktree 生命周期或 Controller 协调；
- Client–Controller 应用契约或 Cloud 租户语义。

`ora-plugin-protocol` 是 frame 实现的参考，但 `ora-node-protocol` 不依赖它。两个 crate 分别拥有
不同协议，必须能够独立演进。

## Crate 结构

```text
crates/node-protocol/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── frame.rs       # 定长分隔的 envelope codec
    ├── identity.rs    # 协议身份值
    ├── message.rs     # 会话和协议消息
    ├── domain.rs      # 协议侧 Node domain 模块
    └── domain/
        └── worktree.rs # 首个垂直切片的命令和结果
```

初始依赖应保持精简：`serde`、`serde_json` 和 Tokio 的异步 I/O 工具。协议 crate 应定义不透明、可
序列化的身份值，而不是依赖 `ora-domain`；领域持久化类型不应成为 wire 兼容性边界。

## Frame codec

初始 codec 参考 `crates/plugin-protocol/src/frame.rs`：

```text
4 字节 big endian 长度
1 字节 frame type
JSON envelope payload
```

长度包含 frame type 字节。codec 必须拒绝零长度 frame、超过最大支持长度的 frame、未知 frame type、
无效 JSON，以及截断的 header 或 payload，并且要在接受 payload 前完成检查。新 frame 开始前遇到
clean EOF 时返回 `None`；遇到不完整 frame 时返回 I/O 错误。

codec 对 `AsyncRead`、`AsyncWrite` 以及可序列化消息类型使用泛型。它不执行请求去重、事件确认、重试
或恢复。并发写入必须由调用方或后续 session adapter 串行化；codec 不能交错两个 frame 的字节。

正式 wire format 从第一版开始就是二进制协议，不额外定义 `JsonDebug` 编码。开发调试在 frame
反序列化为 typed value 后使用 `ora_logging` 的 trace 事件。第一版可以在 TRACE 级别直接打印
typed value；敏感值脱敏暂不纳入本切片。解码失败时还没有 typed value，只记录 frame 元数据和错误
分类。

frame envelope 将协议元数据与 payload 分开承载：

```text
protocol_version
message_type
request_id       （关联 Client 命令时使用）
operation_id     （关联业务操作时使用）
execution_id     （关联 Node 执行尝试时使用）
sequence         （关联有序事件时使用）
payload
```

`operation_id` 和 `execution_id` 是不同的身份。网络重试、重新领取协调工作或进程重启都保留同一执行
身份。codec 保留这些值，但不决定某个操作是否允许重试。

第一版可以让所有合法 Node 消息使用同一个单字节 frame type。保留 type 字段可以为未来的 frame
类别留出空间，同时不把 codec 绑定到 Worktree 消息 enum。

## 首批协议消息

第一版消息集合只覆盖 Worktree 闭环所需的会话和执行契约：

```text
Controller → Node:
  Hello
  EnsureWorktree
  RemoveWorktree
  GetExecutionStatus
  EventAck

Node → Controller:
  HelloAccepted
  Heartbeat
  ExecutionStatus
  WorktreeReady
  WorktreeFailed
  WorktreeRemoved
  WorktreeRemovalFailed
```

`Hello` 携带 Controller 身份和支持的协议版本；`HelloAccepted` 选定版本并返回持久
Node 身份、本次运行实例和能力集。`Heartbeat` 证明会话活性，但不表示某个执行
成功或失败。本 PR 只定义和验证这些 typed contract；握手状态机、超时、心跳调度和
重连行为属于后续 session 和 Transport 实现。

每个 Worktree 命令通过 envelope 携带操作和执行身份，payload 携带目标 Node 身份、
`workspace_id`、`worktree_id`、RepositoryRef、Main Workspace 绑定、base ref、期望分支和
路径策略。结果通过 envelope 关联操作、执行和 sequence，payload 携带 Node 身份、
Node incarnation、outcome，以及实际 worktree 路径、分支和 base commit 等 Node-scoped
事实。绝对路径是 Node 作用域内的事实，不能被解释为 Controller 或 Client 路径。

`GetExecutionStatus` 使用原有操作和执行身份对账。`ExecutionStatus` 用 enum 表示
`Unknown`、`Accepted`、`Running` 或带原终态结果的 `Completed`，不使用多个可选字段组合状态。
`Unknown` 只表示 Node 没有足够证据回答，不允许调用方因此更换身份重试。
`EventAck` 确认精确的 `(execution_id, sequence)`；确认前持久化和确认后清理重放记录的
行为不在本 PR 实现。

状态查询与事件交付的职责遵循
[协议根决策 D4](../../specs/decisions/node/protocol/0-controller-node-protocol.md#d4身份能力和会话恢复)：
`ExecutionStatus` 不携带 `sequence`，即使返回 `Completed` 也不构成事件交付或确认。
会话恢复后，Node 主动重放未确认的原事件；Controller 持久接管后，用该事件的
`(execution_id, sequence)` 发送 `EventAck`。重放不依赖先查询状态，查询也不会停止重放。
查询回复与原事件任意先后到达均不能重复触发业务后续处理；确认丢失时依据持久记录重新确认。
已确认并清理的事件不会因后续状态查询重新进入重放，查询回复也不需要另行确认。
这些约束保留单一的事件交付与确认路径，而不是让状态查询兼任事件交付。

公开接口使用分开的 Controller-to-Node 和 Node-to-Controller 消息 enum。所有消息都是
显式 variant，不能塞进无类型 JSON payload。方向错误、`message_type` 与 payload variant
不匹配、或某类消息缺少必需 envelope 身份时，都必须在协议层被拒绝。

## 身份规则

crate 定义以下可序列化的不透明值：

| 身份                | 含义                                     |
| ------------------- | ---------------------------------------- |
| `ControllerId`      | 建立会话的 Controller 持久身份           |
| `NodeId`            | 执行 Node 的持久身份                     |
| `NodeIncarnationId` | Node 的一次运行实例                      |
| `RequestId`         | 一次逻辑 Client 命令（命令被透传时使用） |
| `OperationId`       | 一次由 Controller 接管的业务操作         |
| `ExecutionId`       | 该操作的一次执行尝试                     |
| `Sequence`          | 执行或事件流内的单调顺序                 |

首个 Worktree 切片中，每个创建或删除操作只建立一次执行尝试。重传使用原有身份和 payload。协议
crate 保留身份和顺序数据；Node 和 Controller 的实现负责持久去重和恢复。

## 首个实现 commit 的验收标准

后续实现 commit 在满足以下条件时完成：

1. `ora-node-protocol` 是一个带有文档化公开接口的 workspace crate。
2. 写入内存 duplex stream 的消息在分片写入后仍可被完整读回。
3. clean EOF、截断输入、超大 frame、未知 frame type、畸形 JSON 和无效消息 envelope 能产生可区分
   的错误。
4. 握手、心跳、Worktree 命令与结果、执行状态查询和结果确认全部通过公开接口
   完成往返，且身份、sequence 和 Node-scoped 事实没有丢失。
5. crate 不依赖 Transport、Git、文件系统、持久化或 `ora-domain`。
6. 测试通过公开 codec 和消息接口验证行为，而不是依赖私有实现细节。
7. 同一身份在各类消息中的序列化表示一致；方向错误、元数据与 payload 不匹配和
   非法执行状态无法被构造或会被解码校验拒绝。

这个 commit 只建立协议 seam。下一个 commit 在该接口之后加入 Node 侧 Worktree 的持久执行和恢复。

“持久执行”表示 Node 在启动外部副作用前，先持久化最小 execution ledger。ledger 保存
`operation_id`、`execution_id`、目标资源、规范化输入、状态、结果或未知标记、事件 sequence 和确认
状态。它不试图让 Git 具备事务能力，也不保存完整的 Node 进程状态。

Node 重启后使用 ledger 和实际 worktree 完成请求去重、未确认结果重放和不明确结果对账。只有
Controller 已经持久接管结果后，结果才允许被确认。如果 Node 在 Git 已改变 worktree、结果尚未
持久化时崩溃，恢复必须先检查资源并报告 `Unknown`，不能盲目再次执行。后续实现 commit 只有在
该协议接口之后测试这些顺序和恢复保证，才算完成。

后续恢复验收还须覆盖：结果发送时断线后的主动重放、查询不停止重放、查询回复与事件乱序、
确认丢失后的重复确认，以及查询已确认执行不重新要求事件交付。这些场景分属 Node、Controller
和 session 的实现责任；本 PR 的消息往返测试只验证契约表达，不证明恢复流程已成立。
