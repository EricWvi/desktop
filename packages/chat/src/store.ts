import type {
  ContractsClient,
  LoadSessionEvent,
  PromptSessionEvent,
  SessionPermissionRequest,
} from "@ora/contracts";
import { createStore, type StoreApi } from "zustand/vanilla";

export type ChatMessageRole = "user" | "assistant";

export interface ChatMessage {
  id: string;
  role: ChatMessageRole;
  content: string;
  createdAt: number;
  stopped?: boolean;
}

export interface SessionConversation {
  messages: ChatMessage[];
  isLoaded: boolean;
  isLoading: boolean;
  isResponding: boolean;
  pendingPermissions: SessionPermissionRequest[];
  error: string | null;
}

export interface SendMessageRequest {
  oraSessionId: string;
  text: string;
}

export interface ChatState {
  conversations: Record<string, SessionConversation>;
  loadSession(oraSessionId: string): Promise<void>;
  sendMessage(request: SendMessageRequest): Promise<void>;
  stopGeneration(oraSessionId: string): void;
  respondToPermission(oraSessionId: string, permissionRequestId: string, optionId: string): Promise<void>;
  clearAll(): void;
  dispose(): void;
}

export interface ChatStoreOptions {
  createId?: () => string;
  now?: () => number;
}

export type ChatStore = StoreApi<ChatState>;
export type ChatSessionClient = Pick<
  ContractsClient["session"],
  "load" | "prompt" | "respondToPermission"
>;

const EMPTY_CONVERSATION: SessionConversation = {
  messages: [],
  isLoaded: false,
  isLoading: false,
  isResponding: false,
  pendingPermissions: [],
  error: null,
};

/** Creates a per-session chat state owner backed directly by generated Ora contracts. */
export function createChatStore(
  client: ChatSessionClient,
  options: ChatStoreOptions = {},
): ChatStore {
  const createId = options.createId ?? (() => crypto.randomUUID());
  const now = options.now ?? Date.now;
  const operations = new Map<string, AbortController>();
  const boundaries = new Set<string>();

  const store = createStore<ChatState>((set, get) => ({
    conversations: {},

    loadSession: async (oraSessionId) => {
      if (operations.has(oraSessionId)) return;
      const previous = get().conversations[oraSessionId] ?? EMPTY_CONVERSATION;
      const controller = new AbortController();
      let staged: SessionConversation = {
        ...EMPTY_CONVERSATION,
        messages: [],
        isLoading: true,
      };
      let stagedBoundary = true;
      let completed = false;
      operations.set(oraSessionId, controller);
      updateConversation(set, oraSessionId, () => ({
        ...previous,
        isLoading: true,
        error: null,
      }));
      try {
        for await (const event of client.load(
          { sessionId: oraSessionId },
          { signal: controller.signal },
        )) {
          if (event.type === "session_update") {
            const result = reduceSessionUpdate(staged, event.update, createId, now, stagedBoundary);
            staged = result.conversation;
            stagedBoundary = result.boundary;
          } else if (event.type === "permission_request") {
            staged = {
              ...staged,
              pendingPermissions: [...staged.pendingPermissions, event],
            };
            stagedBoundary = true;
          } else {
            completed = true;
            stagedBoundary = true;
          }
        }
        if (!completed) {
          throw new Error("agent session load ended before completion");
        }
        boundaries.add(oraSessionId);
        updateConversation(set, oraSessionId, () => ({
          ...staged,
          isLoaded: true,
          isLoading: false,
        }));
      } catch (error) {
        updateConversation(set, oraSessionId, () => ({
          ...previous,
          error: isAbortError(error) ? previous.error : errorMessage(error),
        }));
        if (!isAbortError(error)) throw error;
      } finally {
        operations.delete(oraSessionId);
        updateConversation(set, oraSessionId, (conversation) => ({
          ...conversation,
          isLoading: false,
        }));
      }
    },

    sendMessage: async ({ oraSessionId, text }) => {
      const content = text.trim();
      if (content === "") return;
      if (operations.has(oraSessionId)) {
        throw new Error("this Ora session is already processing an operation");
      }
      const controller = new AbortController();
      operations.set(oraSessionId, controller);
      boundaries.add(oraSessionId);
      updateConversation(set, oraSessionId, (conversation) => ({
        ...conversation,
        messages: [...conversation.messages, {
          id: createId(),
          role: "user",
          content,
          createdAt: now(),
        }],
        isResponding: true,
        error: null,
      }));
      try {
        for await (const event of client.prompt(
          { sessionId: oraSessionId, text: content },
          { signal: controller.signal },
        )) {
          applyPromptEvent(store, oraSessionId, event, createId, now, boundaries);
        }
      } catch (error) {
        if (isAbortError(error)) {
          markLastAssistantStopped(set, oraSessionId);
          clearPendingPermissions(set, oraSessionId);
        } else {
          updateConversation(set, oraSessionId, (conversation) => ({
            ...conversation,
            error: errorMessage(error),
          }));
          throw error;
        }
      } finally {
        operations.delete(oraSessionId);
        boundaries.add(oraSessionId);
        updateConversation(set, oraSessionId, (conversation) => ({
          ...conversation,
          isResponding: false,
        }));
      }
    },

    stopGeneration: (oraSessionId) => operations.get(oraSessionId)?.abort(),

    respondToPermission: async (oraSessionId, permissionRequestId, optionId) => {
      try {
        await client.respondToPermission({
          sessionId: oraSessionId,
          permissionRequestId,
          optionId,
        });
        updateConversation(set, oraSessionId, (conversation) => ({
          ...conversation,
          pendingPermissions: conversation.pendingPermissions.filter(
            (request) => request.permissionRequestId !== permissionRequestId,
          ),
          error: null,
        }));
      } catch (error) {
        updateConversation(set, oraSessionId, (conversation) => ({
          ...conversation,
          error: errorMessage(error),
        }));
        throw error;
      }
    },

    clearAll: () => set({ conversations: {} }),
    dispose: () => {
      operations.forEach((controller) => controller.abort());
      operations.clear();
      boundaries.clear();
    },
  }));

  return store;
}

/** Applies one prompt event and preserves the provider stop reason as a message boundary. */
function applyPromptEvent(
  store: ChatStore,
  oraSessionId: string,
  event: PromptSessionEvent,
  createId: () => string,
  now: () => number,
  boundaries: Set<string>,
): void {
  if (event.type === "session_update") {
    applySessionUpdate(store, oraSessionId, event.update, createId, now, boundaries);
  } else if (event.type === "permission_request") {
    boundaries.add(oraSessionId);
    appendPermission(store.setState, oraSessionId, event);
  } else {
    boundaries.add(oraSessionId);
  }
}

/** Converts only user-visible text chunks into local messages while retaining all other boundaries. */
function applySessionUpdate(
  store: ChatStore,
  oraSessionId: string,
  update: Extract<LoadSessionEvent, { type: "session_update" }>["update"],
  createId: () => string,
  now: () => number,
  boundaries: Set<string>,
): void {
  updateConversation(store.setState, oraSessionId, (conversation) => {
    const result = reduceSessionUpdate(
      conversation,
      update,
      createId,
      now,
      boundaries.has(oraSessionId),
    );
    if (result.boundary) boundaries.add(oraSessionId);
    else boundaries.delete(oraSessionId);
    return result.conversation;
  });
}

/** Reduces one provider update without exposing staged history through the live store. */
function reduceSessionUpdate(
  conversation: SessionConversation,
  update: Extract<LoadSessionEvent, { type: "session_update" }>["update"],
  createId: () => string,
  now: () => number,
  boundary: boolean,
): { conversation: SessionConversation; boundary: boolean } {
  if (
    update.sessionUpdate !== "user_message_chunk" &&
    update.sessionUpdate !== "agent_message_chunk"
  ) {
    return { conversation, boundary: true };
  }
  if (update.content.type !== "text") {
    return { conversation, boundary: true };
  }
  const role: ChatMessageRole = update.sessionUpdate === "user_message_chunk"
    ? "user"
    : "assistant";
  const messages = [...conversation.messages];
  const last = messages.at(-1);
  const continuesSameMessage = update.messageId == null || last?.id === update.messageId;
  if (!boundary && last?.role === role && continuesSameMessage) {
    messages[messages.length - 1] = { ...last, content: last.content + update.content.text };
  } else {
    messages.push({
      id: update.messageId ?? createId(),
      role,
      content: update.content.text,
      createdAt: now(),
    });
  }
  return { conversation: { ...conversation, messages }, boundary: false };
}

function appendPermission(
  set: ChatStore["setState"],
  oraSessionId: string,
  request: SessionPermissionRequest,
): void {
  updateConversation(set, oraSessionId, (conversation) => ({
    ...conversation,
    pendingPermissions: [...conversation.pendingPermissions, request],
  }));
}

function markLastAssistantStopped(set: ChatStore["setState"], oraSessionId: string): void {
  updateConversation(set, oraSessionId, (conversation) => {
    const messages = [...conversation.messages];
    let index = messages.length - 1;
    while (index >= 0 && messages[index]?.role !== "assistant") index -= 1;
    if (index >= 0) messages[index] = { ...messages[index]!, stopped: true };
    return { ...conversation, messages };
  });
}

/** Clears requests that the backend settles as cancelled with the aborted prompt. */
function clearPendingPermissions(set: ChatStore["setState"], oraSessionId: string): void {
  updateConversation(set, oraSessionId, (conversation) => ({
    ...conversation,
    pendingPermissions: [],
  }));
}

function updateConversation(
  set: ChatStore["setState"],
  oraSessionId: string,
  update: (conversation: SessionConversation) => SessionConversation,
): void {
  set((state) => ({
    conversations: {
      ...state.conversations,
      [oraSessionId]: update(state.conversations[oraSessionId] ?? EMPTY_CONVERSATION),
    },
  }));
}

function isAbortError(error: unknown): boolean {
  return error instanceof Error && error.name === "AbortError";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Agent request failed";
}
