import { useState, useEffect, useCallback } from "react";
import { ContractsClientProvider, useContractsClient } from "../context/contracts-context";
import { useTasks } from "../hooks/use-tasks";
import { useSessions } from "../hooks/use-sessions";
import { TaskListSidebar } from "../components/task-list-sidebar";
import { TaskDetail } from "../components/task-detail";
import { TerminalPanel } from "../components/terminal-panel";
import { CreateTaskDialog } from "../components/create-task-dialog";
import { ResizablePanelGroup, ResizablePanel, ResizableHandle, Spinner } from "@ora/ui";
import type { Project } from "@ora/contracts";

function AppShellInner() {
  const client = useContractsClient();
  const [project, setProject] = useState<Project | null>(null);
  const [projectLoading, setProjectLoading] = useState(true);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);

  const { tasks, createTask, deleteTask } = useTasks(project?.id ?? null);
  const { createTerminalSession, getTerminalSession } = useSessions();

  useEffect(() => {
    client
      .listProjects({})
      .then((res) => {
        if (res.projects.length > 0) setProject(res.projects[0]);
      })
      .finally(() => setProjectLoading(false));
  }, [client]);

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) ?? null;
  const terminalSession = selectedTaskId ? getTerminalSession(selectedTaskId) ?? null : null;

  const handleCreateTask = useCallback(
    async (title: string) => {
      const task = await createTask(title);
      if (task) setSelectedTaskId(task.id);
    },
    [createTask],
  );

  const handleDeleteTask = useCallback(
    async (taskId: string) => {
      await deleteTask(taskId);
      if (selectedTaskId === taskId) setSelectedTaskId(null);
    },
    [deleteTask, selectedTaskId],
  );

  const handleStartTerminal = useCallback(
    async (taskId: string) => {
      await createTerminalSession(taskId);
    },
    [createTerminalSession],
  );

  const handleStopTerminal = useCallback(() => {
    // Terminal will be cleaned up by killing the WebSocket; session stays in list
  }, []);

  if (projectLoading) {
    return (
      <div className="flex h-screen w-screen items-center justify-center">
        <Spinner className="h-6 w-6" />
      </div>
    );
  }

  return (
    <>
      <ResizablePanelGroup orientation="horizontal" className="h-screen w-screen">
        <ResizablePanel defaultSize="20" minSize="15" maxSize="35">
          <TaskListSidebar
            tasks={tasks}
            selectedTaskId={selectedTaskId}
            onSelectTask={setSelectedTaskId}
            onCreateClick={() => setCreateDialogOpen(true)}
            onDeleteTask={handleDeleteTask}
          />
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize="80">
          <ResizablePanelGroup orientation="vertical">
            <ResizablePanel defaultSize={terminalSession ? "40" : "100"}>
              <TaskDetail
                task={selectedTask}
                terminalSession={terminalSession}
                onStartTerminal={handleStartTerminal}
                onStopTerminal={handleStopTerminal}
                onDeleteTask={handleDeleteTask}
              />
            </ResizablePanel>
            {terminalSession && (
              <>
                <ResizableHandle withHandle />
                <ResizablePanel defaultSize="60" minSize="20">
                  <TerminalPanel sessionId={terminalSession.id} visible />
                </ResizablePanel>
              </>
            )}
          </ResizablePanelGroup>
        </ResizablePanel>
      </ResizablePanelGroup>

      <CreateTaskDialog
        open={createDialogOpen}
        onClose={() => setCreateDialogOpen(false)}
        onSubmit={handleCreateTask}
      />
    </>
  );
}

export function AppShell() {
  return (
    <ContractsClientProvider>
      <AppShellInner />
    </ContractsClientProvider>
  );
}
