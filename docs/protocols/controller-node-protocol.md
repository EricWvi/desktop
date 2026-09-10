# Controller–Node Protocol

English | [中文](controller-node-protocol.zh.md)

This document is the implementation plan for the
`feat(node-protocol): define worktree execution contracts` PR and lands as its first docs commit.
The PR adds a new `ora-node-protocol` crate and defines the session, execution, and framing
contracts needed by the local Worktree loop. It does not implement a transport, start a Node
process, execute Git operations, or add Controller persistence. A final docs commit in the PR will
remove plan-oriented language and synchronize this document with the implemented interface.

The first loop targets Desktop, one local Node, and a Workspace whose Main Workspace already exists
on that Node. Cloud, SSH, and other transports reuse this protocol after the local slice is proven.

## Module seam

`ora-node-protocol` is the shared protocol seam between Controller and Node. Both sides depend on
its public message and framing interface; transport adapters depend on the same interface without
changing message semantics.

The crate owns:

- protocol identities and version values;
- session handshake, heartbeat, execution-status query, and result-acknowledgement messages;
- message envelopes and Worktree command/result types;
- the protocol-side Node domain model used by those commands and results;
- length-delimited frame encoding and decoding;
- protocol-level validation errors;
- tests that prove fragmented I/O, malformed frames, and message round trips.

The crate does not own:

- Unix sockets, Windows Named Pipes, stdio process management, SSH, or TLS;
- Node startup, registration persistence, event replay storage, or deduplication state;
- Git, filesystem, worktree lifecycle, or Controller reconciliation;
- Client–Controller application contracts or Cloud tenant semantics.

`ora-plugin-protocol` is the reference for the frame implementation, but `ora-node-protocol` does
not depend on it. The two crates have different protocol ownership and must be able to evolve
independently.

## Crate shape

```text
crates/node-protocol/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── frame.rs       # length-delimited envelope codec
    ├── identity.rs    # protocol identity values
    ├── message.rs     # session and protocol messages
    ├── domain.rs      # protocol-side Node domain module
    └── domain/
        └── worktree.rs # first vertical slice commands and results
```

The initial dependency set should remain small: `serde`, `serde_json`, and Tokio's async I/O
utilities. The protocol crate should define opaque, serializable identity values instead of
depending on `ora-domain`; domain persistence types must not become a wire compatibility boundary.

## Frame codec

The initial codec follows `crates/plugin-protocol/src/frame.rs`:

```text
4-byte big-endian length
1-byte frame type
JSON envelope payload
```

The length includes the frame type byte. The codec must reject zero-length frames, frames above the
maximum supported size, unknown frame types, invalid JSON, and truncated headers or payloads before
the payload is accepted. A clean EOF before a new frame returns `None`; a partial frame returns an
I/O error.

The codec is generic over `AsyncRead` and `AsyncWrite` and over serializable message types. It does
not perform request deduplication, event acknowledgement, retries, or recovery. Concurrent writers
must be serialized by the caller or by a later session adapter; the codec must never interleave
bytes from two frames.

The production wire format is binary from the first implementation. A separate `JsonDebug` encoding
is not part of the protocol. Development diagnostics use `ora_logging` trace events after a frame has
been decoded into a typed value. The initial implementation may print the typed value directly at
TRACE; sensitive-value redaction is intentionally out of scope for this slice. Decode failures log
frame metadata and the error classification because no typed value exists yet.

The frame envelope carries protocol metadata separately from its payload:

```text
protocol_version
message_type
request_id       (when associated with a Client command)
operation_id     (when associated with a business operation)
execution_id     (when associated with a Node execution attempt)
sequence         (when associated with an ordered event)
payload
```

`operation_id` and `execution_id` are distinct. A network retry, coordination re-claim, or process
restart keeps the same execution identity. The codec preserves these values but does not decide
whether an operation may be retried.

The first implementation may use the same one-byte frame type for every valid Node message. Keeping
the type field in the envelope leaves room for future frame classes without coupling the codec to
the Worktree message enum.

## Initial protocol messages

The first message set is intentionally limited to the session and execution contracts required by
the Worktree loop:

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

`Hello` carries the persistent Controller identity and supported protocol versions;
`HelloAccepted` selects a version and returns the persistent Node identity, current incarnation,
and capability set. `Heartbeat` proves session liveness but says nothing about whether an execution
has succeeded or failed. This PR defines and verifies these typed contracts only; the handshake
state machine, timeouts, heartbeat scheduling, and reconnection behaviour belong to later session
and transport implementations.

Each Worktree command carries its operation and execution identities in the envelope. Its payload
carries the target Node identity, `workspace_id`, `worktree_id`, RepositoryRef, Main Workspace
binding, base ref, expected branch, and path policy. A result is correlated with its operation,
execution, and sequence through the envelope. Its payload carries the Node identity, Node
incarnation, outcome, and Node-scoped facts such as the actual worktree path, branch, and base
commit. Absolute paths are facts scoped to the Node; they are never interpreted as Controller or
Client paths.

`GetExecutionStatus` reconciles using the original operation and execution identities.
`ExecutionStatus` uses an enum to represent `Unknown`, `Accepted`, `Running`, or `Completed` with
the original terminal result; it does not encode state through combinations of optional fields.
The outer `ExecutionStatus.node` identifies the current reporter. A `Completed` result retains its
original Node runtime identity: its `NodeId` must match the reporter, while its `NodeIncarnationId`
may differ after a restart. Both public codec directions reject a mismatch with
`CompletedNodeMismatch`. Session/Controller code must additionally verify the reporter against the
session binding and the execution's dispatched Node; internal consistency does not establish trust.
`Unknown` means only that the Node lacks sufficient evidence to answer and does not permit the
caller to retry under new identities. `EventAck` acknowledges one exact
`(execution_id, sequence)` pair. Persist-before-ack behaviour and replay-record cleanup are not
implemented in this PR.

Status reconciliation and event delivery follow
[protocol decision D4](../../specs/decisions/node/protocol/0-controller-node-protocol.md#d4身份能力和会话恢复).
`ExecutionStatus` carries no `sequence`; even a `Completed` reply is neither event delivery nor an
acknowledgement. After session recovery, Node actively replays original unacknowledged events.
Controller sends `EventAck` for the original `(execution_id, sequence)` only after durable acceptance.
Replay does not depend on a preceding status query, and querying does not stop replay. Status replies
and original events may arrive in either order without triggering duplicate downstream business
processing. A lost acknowledgement is sent again using durable records. Querying an already
acknowledged execution does not restart replay of a cleaned-up event or require another acknowledgement.
These rules retain one event delivery and acknowledgement path instead of making status queries a
second delivery path.

The public interface uses separate Controller-to-Node and Node-to-Controller message enums. Every
message is an explicit variant rather than an untyped JSON payload. A wrong-direction message, a
`message_type` that disagrees with the payload variant, or a message missing envelope identities
required for that variant must be rejected at the protocol layer.

## Identity rules

The crate defines serializable opaque values for:

| Identity            | Meaning                                                    |
| ------------------- | ---------------------------------------------------------- |
| `ControllerId`      | Persistent identity of the Controller opening the session  |
| `NodeId`            | Persistent identity of an execution Node                   |
| `NodeIncarnationId` | One running instance of a Node                             |
| `RequestId`         | One logical Client command, when the command is propagated |
| `OperationId`       | One Controller-owned business operation                    |
| `ExecutionId`       | One execution attempt for that operation                   |
| `Sequence`          | Monotonic ordering within an execution/event stream        |

The first Worktree slice creates one execution attempt per create or remove operation. Retransmission
uses the original identities and payload. The protocol crate preserves identity and ordering data;
Node and Controller implementations enforce durable deduplication and recovery.

## Acceptance criteria for the first implementation commit

The implementation commit that follows this plan is complete when:

1. `ora-node-protocol` is a workspace crate with a documented public interface.
2. A message written to an in-memory duplex stream can be read back after fragmented writes.
3. Clean EOF, truncated input, oversized frames, unknown frame types, malformed JSON, and invalid
   message envelopes produce distinguishable errors.
4. Handshake, heartbeat, Worktree commands and results, execution-status queries, and result
   acknowledgements all round-trip through the public interface without losing identities,
   sequence, or Node-scoped facts.
5. The crate has no transport, Git, filesystem, persistence, or `ora-domain` dependency.
6. Tests exercise the public codec and message interface rather than private implementation details.
7. Every identity has one consistent serialized representation across message kinds; wrong
   directions, metadata/payload mismatches, and illegal execution states are unrepresentable or
   rejected during decoding validation.

This commit establishes the protocol seam only. The next commit adds durable Node-side Worktree
execution and recovery behind this interface.

“Durable execution” means that Node persists the minimum execution ledger before starting an external
side effect. The ledger keeps the `operation_id`, `execution_id`, target resource, normalized input,
state, result or unknown marker, event sequence, and acknowledgement state. It does not attempt to
make Git transactional or persist the complete Node process.

After a restart, Node uses the ledger and the actual worktree to deduplicate requests, replay
unacknowledged results, and reconcile ambiguous outcomes. A result is acknowledged only after the
Controller has durably accepted it. If Node crashes after Git has changed the worktree but before the
result is persisted, recovery reports `Unknown` until the resource is checked; it must not blindly
run the operation again. The next implementation commit is complete only when these ordering and
recovery guarantees are tested behind this protocol interface.

Later recovery acceptance must also cover active replay after disconnection during result delivery,
queries not stopping replay, either arrival order of status replies and events, repeated acknowledgement
after a lost ACK, and queries of acknowledged executions not requiring event redelivery. These scenarios
belong to the Node, Controller, and session implementations. This PR's message round trips verify the
contract representation, not that the recovery flow already works.
