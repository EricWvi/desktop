import React, { useCallback, useEffect, useRef, useState } from "react";
import type { ContractsClient, Session, Task } from "@ora/contracts";
import { Button } from "@ora/ui";
import { Plus } from "lucide-react";
import { TaskSidebar } from "./task-sidebar";
import { ActivityBar } from "./activity-bar";
import { TerminalPanel } from "./terminal-panel";

interface AppShellProps {
  client: ContractsClient;
}

export function AppShell({ client }: AppShellProps) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [visibleSessionId, setVisibleSessionId] = useState<string | null>(null);
  const [isCreatingTerminal, setIsCreatingTerminal] = useState(false);

  // Load tasks on mount.
  useEffect(() => {
    client.listTasks({}).then(({ tasks: loaded }) => {
      setTasks(loaded);
      if (loaded.length > 0) {
        setSelectedTaskId(loaded[0]!.id);
      }
    });
  }, [client]);

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) ?? null;
  const taskSessions = sessions.filter((s) => s.taskId === selectedTaskId);

  const handleNewTerminal = useCallback(async () => {
    if (!selectedTaskId || isCreatingTerminal) return;
    setIsCreatingTerminal(true);
    try {
      // cols/rows are approximate; the terminal will send a real resize once
      // the DOM has been measured.
      const { session } = await client.createSession({
        taskId: selectedTaskId,
        agentId: "terminal",
        agentSessionId: null,
        status: "running",
        terminal: { cols: 120, rows: 30 },
      });
      setSessions((prev) => [...prev, session]);
      setVisibleSessionId(session.id);
    } finally {
      setIsCreatingTerminal(false);
    }
  }, [client, selectedTaskId, isCreatingTerminal]);

  const handleToggleSession = useCallback((sessionId: string) => {
    setVisibleSessionId((prev) => (prev === sessionId ? null : sessionId));
  }, []);

  // Reset visible session when the selected task changes so we don't show a
  // terminal that belongs to a different task.
  const prevSelectedTaskId = useRef(selectedTaskId);
  useEffect(() => {
    if (prevSelectedTaskId.current !== selectedTaskId) {
      prevSelectedTaskId.current = selectedTaskId;
      setVisibleSessionId(null);
    }
  }, [selectedTaskId]);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      <TaskSidebar
        tasks={tasks}
        selectedTaskId={selectedTaskId}
        onSelect={setSelectedTaskId}
      />

      <main className="flex-1 flex flex-col min-w-0 min-h-0">
        <header className="h-10 flex items-center justify-between px-4 border-b shrink-0">
          <span className="text-[13px] font-medium truncate">
            {selectedTask?.title ?? "Select a task"}
          </span>
          <Button
            variant="ghost"
            size="sm"
            onClick={handleNewTerminal}
            disabled={!selectedTaskId || isCreatingTerminal}
          >
            <Plus size={13} />
            New Terminal
          </Button>
        </header>

        {/*
         * Terminal panels are rendered for every session but only the visible
         * one is shown. Keeping them mounted preserves scroll history and the
         * live PTY connection when switching between terminals.
         */}
        <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
          {taskSessions.length === 0 && (
            <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
              No terminal open
            </div>
          )}
          {taskSessions.map((session) => (
            <TerminalPanel
              key={session.id}
              sessionId={session.id}
              isVisible={session.id === visibleSessionId}
            />
          ))}
        </div>
      </main>

      <ActivityBar
        sessions={taskSessions}
        visibleSessionId={visibleSessionId}
        onToggle={handleToggleSession}
      />
    </div>
  );
}
