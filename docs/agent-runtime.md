# ACP Agent Runtime

`ora-backend` starts one supervised `opencode acp` child for each Backend instance. Every persisted Ora Session owns a serialized actor, but actors share the application-scoped ACP connection and route events by the private provider session id. One Session accepts only one load or prompt operation at a time while different Sessions remain concurrent.

## Process and Session Lifecycle

- OpenCode is the only agent runtime. Session contracts and persistence do not carry a per-session CLI selection.
- The shared child starts in the user's home directory with the single `acp` argument and piped stdin, stdout, and stderr. Session setup requests carry the owning Task worktree as `cwd`.
- Task worktrees resolve through Task → stored Worktree id → stored branch name → Git's authoritative worktree metadata. A configured worktree creation root is never used to reconstruct an existing path.
- Backend startup reconciles stale Running rows to Stopped, then a dedicated runtime thread starts OpenCode and performs `initialize`. Owning the runtime here is necessary because synchronous Desktop bootstrap does not guarantee an ambient Tokio runtime. Startup failures leave Ora available, and the supervisor retries with capped exponential backoff.
- Create calls `session/new` on the ready shared connection and persists the Ora Session only after setup succeeds. The guarded insert fails if its Task was deleted while the handshake was in flight.
- Load registers a route on the current connection generation, marks the row Running, and calls `session/load` with the private `agentSessionId`. Every setup or replay failure restores Stopped.
- Connection loss fails all in-flight operations, marks every registered Session Stopped, terminates and reaps the old process tree, and only then starts a replacement. Sessions are loaded again only on demand; prompts are never replayed automatically.

## Flow Control

ACP stdout is newline-delimited JSON-RPC with an 8 MiB frame limit. The connection reader uses an unbounded handoff to the always-running central router, while each registered Session owns a bounded 256-item update queue and an independent control queue. This keeps connection-wide parsing from imposing one Session's backpressure on another. A per-Session overflow stops only the affected Session; no data is silently discarded.

Unknown agent-originated JSON-RPC requests receive a correlated `-32601` method-not-found response and do not terminate the connection. Malformed frames, unmatched responses, oversized frames, and stdio loss are connection failures. Updates for unloaded Sessions are treated as stale and discarded rather than taking down unrelated work.

Dropping a Web body, closing a Tauri stream, or aborting the frontend `AsyncIterable` sends `session/cancel`. Prompt cancellation waits up to five seconds for the provider to settle. A session-level timeout unloads and stops only that Session; it never restarts the shared process. Explicit Stop optionally calls `session/close` when advertised, unloads the route, and preserves provider history for a later load.

## Ownership Boundaries

Ora deletion removes only Ora-owned database records. It does not call ACP session delete and does not touch Git branches or worktrees. Session deletion serializes against new actor operations, unloads its route, and then soft-deletes the row under the same lifecycle guard. Task and Project deletion reject Running descendants and transactionally cascade stopped Ora records.

Dropping the last Backend owner asks the supervisor to stop accepting work, cancels routed operations, and initiates bounded termination and reaping of the OpenCode process tree. The process remains alive while the Backend exists even when no Sessions are registered.
