# Frontend ownership

## Translation resources

Feature copy lives in data-only `features/<feature>/translations.ts` entries. Settings splits
general settings, plugins, Skills, and Roles behind its data-only translation entry; workflow
editor copy belongs to the editor even when its historical keys start with `settings.workflow`.
Domain-specific contract errors live beside the corresponding feature's copy. Only truly shared
shell/transport messages remain in `i18n/common-resources.ts`.

`i18n/resources.ts` explicitly composes those entries. It imports no React or i18next integration,
and resources never import their feature implementation. `composeTranslationResources` rejects
duplicate logical-key ownership, missing language keys, and incomplete plural groups. English
`_one` / `_other` variants and Chinese unsuffixed count messages retain their original keys; the
check compares logical keys instead of demanding identical raw plural suffixes.

`i18n/i18n-instance.ts` is still the sole `appI18n` initializer. It composes all copy synchronously
(`initAsync: false`), retains Chinese fallback, and preserves `ora.locale` storage and document
language behavior. Features do not dynamically register resources when mounting. Tests that render
`useTranslation` import `appI18n` themselves rather than depending on another file in the worker.

Adding or removing a feature changes its resource entry and the explicit composition. Editing
existing copy changes only its owner. `resources.test.ts` exercises data-only composition,
ownership collisions, language parity, and plural validation; `i18n-instance.test.ts` covers
synchronous availability, language switching, plurals, and blocked storage.

## Query and invalidation ownership

`state/data/` owns cache identity and shared data rules. The central `state/hooks/query-keys.ts`
has been removed. Consumers import the relevant owner directly; there is no replacement global
barrel or key registry. The existing 37 factories retain their exact tuples, including historical
spelling, prefix structure, task ids in `workspace-files`, and nullable model-selector arguments.

| Owner                                                                   | Responsibility                                                                                           |
| ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `workspace.ts`, `sessions.ts`                                           | Shared lists, authoritative response adoption, aggregate-deletion scrubbing, and session tree placement  |
| `workflows.ts`, `workflow-runs.ts`                                      | Definition/draft/version queries and persisted run queries, mutations, and projections                   |
| `mock-workflows.ts`, `mock-workflow-runs.ts`, `mock-workflow-mounts.ts` | Memory-runtime identities, subscriptions, and mutations; separate from persisted run caches              |
| `files.ts`, `file-watch.ts`, `diff.ts`                                  | Scoped file access, file-event invalidation/reconnection, and workspace diff invalidation                |
| `plugins.ts`, `agent-runtime.ts`, `plugin-lifecycle.ts`                 | Catalog/configuration caches, per-agent model prefixes, and explicit cross-domain lifecycle coordination |
| `agents.ts`, `skills.ts`, `settings.ts`, `identity.ts`, `effects.ts`    | Definition, preference, identity, and effect cache identities owned by those data domains                |

UI hooks still own selection, draft/composer cleanup, mutation activity, and view-specific actions.
They apply authoritative cache results through the data interface before coordinating UI state.
Project deletion includes main-workspace sessions and task sessions even when the workspace-list
cache has not loaded the task workspace. List renames patch responses and invalidate with
`refetchType: "none"`; standalone deletes refresh active lists, while parent cascades may defer
child refreshes. This distinction must not be replaced with a blanket invalidation.

Plugin refresh scopes are deliberately different:

| Trigger                      | Invalidated queries                                                                           |
| ---------------------------- | --------------------------------------------------------------------------------------------- |
| Agent activation             | Runtime availability and that agent's model queries in every workspace                        |
| Agent stop/removal           | Runtime availability, **not** model discovery against the stopped runtime                     |
| Settled plugin mutation      | Installed and available plugins                                                               |
| External plugin-status event | Installed plugins, runtime availability, and Skills; not available plugins or model discovery |
| Agent-model event            | Only that agent's model-query prefix                                                          |

The application event hook owns reconnect/abort state, but delegates those cache rules. A ready
event or a disconnect explicitly **refetches** sessions to close missed-event gaps; a title event
invalidates them. Promise-returning mutation callbacks still await refresh completion, while event
consumption does not block on invalidation.

Persisted detail keys remain `['workflowRun', 'detail', runId]`; the memory runtime keeps
`['workflowRun', runId]`. Neither list invalidation nor detail removal may merge those spaces.
The runtime provider and its injected context belong to shell composition, not workflow-run UI.
Terminal-status classification lives in `@ora/workflow-runtime`; data does not import UI chrome.

Files retains the explicit Explorer/Search product composition. Its data interface owns the
task-worktree/project-checkout choice, keys, and refresh scope. Rename events invalidate both
paths and parent directories plus searches; rescan invalidates the whole selected scope, never a
neighboring checkout. Changing scope aborts the old watcher and unmount aborts the current watcher.

Data tests use real QueryClient caches/observers to verify exact invalidation and refetch behavior.
The Files scope-switch test awaits rendered listings and actual stream finalization, and the
existing mutation and event tests continue to exercise UI coordination and reconnect behavior.

## Typed test transport

`test/contracts-transport.ts` supplies `createTestClient(handlers)`, which uses the production
generated client. `TestHandlers` derives each request, response, and unary/stream mode from the
operation catalog; fixtures do not spell another namespace/member map. Tests register operation
handlers explicitly, composing domain adapters or overriding a specific operation in that object.
The transport rejects omitted/unknown operations and wrong response modes. Stream lookup happens
on first consumption; iterator exit forwards cleanup to the handler. DTOs, bigint values, and
AbortSignal options cross this seam unchanged.

`test/memory/` owns domain-specific state, constructors, and operation handlers. Stateful
workspace/session, plugin, definition, settings, and persisted-run adapters register typed
operations directly. The explicit `emptyFilesHandlers`, `readyEffectHandlers`, identity, and
app-event fixtures describe their synthetic behavior in their names/modules; they are not global
defaults. A test can use one domain without implicitly configuring any other:

```typescript
const state = createWorkspaceMemory();
const client = createTestClient(workspaceHandlers(state));
```

The Files scope-switch test instead registers exactly four individual handlers. Project/task,
Agent/Skill, and plugin-list hook tests select their adapters explicitly. Other test surfaces
compose their needed domains in local fixture constructors. Thirteen test files need no default
operations at all and start with `createTestClient({})`. The old `test/mock-client.ts` all-domain
composer has been deleted, including its combined state type. New feature fixtures change their
local composition and data-owner adapters, not a global mock-client registry.

Existing direct client-method overrides are being migrated to the handler seam as the remaining
stage 5c work. New tests should supply their custom behavior as typed operation handlers instead
of replacing generated client methods.
