import { createTestClient } from "./contracts-transport";
import {
  workspaceHandlers,
  createWorkspaceMemory,
  type WorkspaceMemoryState,
} from "./memory/workspaces";
import {
  sessionHandlers,
  createSessionMemory,
  type SessionMemoryState,
} from "./memory/sessions";
import {
  agentRuntimeHandlers,
  createAgentRuntimeMemory,
  type AgentRuntimeMemoryState,
} from "./memory/agent-runtime";
import {
  pluginHandlers,
  createPluginMemory,
  type PluginMemoryState,
} from "./memory/plugins";
import {
  agentHandlers,
  createAgentMemory,
  type AgentMemoryState,
} from "./memory/agents";
import {
  skillHandlers,
  createSkillMemory,
  type SkillMemoryState,
} from "./memory/skills";
import {
  settingsHandlers,
  createSettingsMemory,
  type SettingsMemoryState,
} from "./memory/settings";
import {
  workflowHandlers,
  createWorkflowMemory,
  type WorkflowMemoryState,
} from "./memory/workflows";
import {
  workflowRunHandlers,
  createWorkflowRunMemory,
  type WorkflowRunMemoryState,
} from "./memory/workflow-runs";
import { emptyFilesHandlers } from "./memory/files";
import { readyEffectHandlers } from "./memory/effects";
import { identityHandlers } from "./memory/identity";
import { appEventHandlers } from "./memory/app-events";
export type { MockWorkflowRecord } from "./memory/workflows";
export type { MockWorkflowRunRecord } from "./memory/workflow-runs";

/** Transitional state composition; migrate tests to their data owners before removing this file. */
export type MockClientState = WorkspaceMemoryState &
  SessionMemoryState &
  AgentRuntimeMemoryState &
  PluginMemoryState &
  AgentMemoryState &
  SkillMemoryState &
  SettingsMemoryState &
  WorkflowMemoryState &
  WorkflowRunMemoryState;

/** Preserves existing fixtures until each test declares the adapters it needs. */
export function createMockClientState(): MockClientState {
  const plugins = createPluginMemory();
  return {
    ...createWorkspaceMemory(),
    ...createSessionMemory(),
    ...createAgentRuntimeMemory(plugins.installedPlugins),
    ...plugins,
    ...createAgentMemory(),
    ...createSkillMemory(),
    ...createSettingsMemory(),
    ...createWorkflowMemory(),
    ...createWorkflowRunMemory(),
  };
}

/** Temporary all-domain composer; every request now uses the generated production client. */
export function createMockClient(state: MockClientState) {
  return createTestClient({
    ...workspaceHandlers(state),
    ...sessionHandlers(state),
    ...agentRuntimeHandlers(state),
    ...pluginHandlers(state),
    ...agentHandlers(state),
    ...skillHandlers(state),
    ...settingsHandlers(state),
    ...workflowHandlers(state),
    ...workflowRunHandlers(state),
    ...emptyFilesHandlers(),
    ...readyEffectHandlers(),
    ...identityHandlers(),
    ...appEventHandlers(),
  });
}
