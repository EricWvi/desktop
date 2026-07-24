import assert from "node:assert/strict";
import test from "node:test";
import type { LoadSessionEvent, PromptSessionEvent } from "@ora/contracts";
import { createChatStore, type ChatSessionClient } from "../src/index.js";

/** Builds one ACP text update without exposing protocol transport details to the tests. */
function textEvent(
  role: "user_message_chunk" | "agent_message_chunk",
  text: string,
  messageId: string,
): LoadSessionEvent {
  return {
    type: "session_update",
    update: {
      sessionUpdate: role,
      messageId,
      content: { type: "text", text },
    },
  };
}

/** Yields a deterministic finite stream in the same shape as the generated client. */
async function* events<Event>(items: Event[]): AsyncIterable<Event> {
  for (const item of items) yield item;
}

test("loads provider history and respects ACP message boundaries", async () => {
  const client: ChatSessionClient = {
    load: () => events([
      textEvent("user_message_chunk", "hel", "user-1"),
      textEvent("user_message_chunk", "lo", "user-1"),
      textEvent("user_message_chunk", "again", "user-2"),
      textEvent("agent_message_chunk", "hi", "agent-1"),
      { type: "completed" },
    ]),
    prompt: () => events<PromptSessionEvent>([]),
    respondToPermission: async () => ({}),
  };
  let nextId = 0;
  const store = createChatStore(client, {
    createId: () => `local-${++nextId}`,
    now: () => 42,
  });

  await store.getState().loadSession("ora-1");

  assert.deepEqual(store.getState().conversations["ora-1"], {
    messages: [
      { id: "user-1", role: "user", content: "hello", createdAt: 42 },
      { id: "user-2", role: "user", content: "again", createdAt: 42 },
      { id: "agent-1", role: "assistant", content: "hi", createdAt: 42 },
    ],
    isLoaded: true,
    isLoading: false,
    isResponding: false,
    pendingPermissions: [],
    error: null,
  });
});

test("aborting a prompt retains and marks the partial assistant response", async () => {
  const client: ChatSessionClient = {
    load: () => events<LoadSessionEvent>([]),
    prompt: (_request, options) => ({
      async *[Symbol.asyncIterator]() {
        yield textEvent("agent_message_chunk", "partial", "agent-1") as PromptSessionEvent;
        yield {
          type: "permission_request",
          permissionRequestId: "permission-1",
          toolCall: { toolCallId: "tool-1", title: "Run command" },
          options: [{ optionId: "allow", name: "Allow", kind: "allow_once" }],
        } satisfies PromptSessionEvent;
        await new Promise<void>((_resolve, reject) => {
          options?.signal?.addEventListener("abort", () => {
            const error = new Error("cancelled");
            error.name = "AbortError";
            reject(error);
          }, { once: true });
        });
      },
    }),
    respondToPermission: async () => ({}),
  };
  const store = createChatStore(client, { createId: () => "user-1", now: () => 42 });
  const sending = store.getState().sendMessage({ oraSessionId: "ora-1", text: " hello " });
  await new Promise<void>((resolve) => setTimeout(resolve, 0));

  store.getState().stopGeneration("ora-1");
  await sending;

  assert.deepEqual(store.getState().conversations["ora-1"]?.messages, [
    { id: "user-1", role: "user", content: "hello", createdAt: 42 },
    { id: "agent-1", role: "assistant", content: "partial", createdAt: 42, stopped: true },
  ]);
  assert.equal(store.getState().conversations["ora-1"]?.isResponding, false);
  assert.deepEqual(store.getState().conversations["ora-1"]?.pendingPermissions, []);
});

test("rolls back staged load updates when replay fails before completion", async () => {
  const client: ChatSessionClient = {
    load: () => ({
      async *[Symbol.asyncIterator]() {
        yield textEvent("agent_message_chunk", "uncommitted", "agent-new");
        throw new Error("load failed");
      },
    }),
    prompt: () => events<PromptSessionEvent>([]),
    respondToPermission: async () => ({}),
  };
  const store = createChatStore(client, { createId: () => "local", now: () => 42 });
  store.setState({
    conversations: {
      "ora-1": {
        messages: [{ id: "old", role: "assistant", content: "history", createdAt: 1 }],
        isLoaded: true,
        isLoading: false,
        isResponding: false,
        pendingPermissions: [],
        error: null,
      },
    },
  });

  await assert.rejects(store.getState().loadSession("ora-1"), /load failed/);

  assert.deepEqual(store.getState().conversations["ora-1"], {
    messages: [{ id: "old", role: "assistant", content: "history", createdAt: 1 }],
    isLoaded: true,
    isLoading: false,
    isResponding: false,
    pendingPermissions: [],
    error: "load failed",
  });
});
