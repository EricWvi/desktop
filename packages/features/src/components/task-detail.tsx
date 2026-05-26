import type { Task, Session } from "@ora/contracts";
import { Badge, Button, Empty, Separator } from "@ora/ui";
import { Terminal, Trash2, Square, ListTodo } from "lucide-react";

interface TaskDetailProps {
  task: Task | null;
  terminalSession: Session | null;
  onStartTerminal: (taskId: string) => void;
  onStopTerminal: () => void;
  onDeleteTask: (taskId: string) => void;
}

const statusBadgeVariant = {
  todo: "outline" as const,
  doing: "default" as const,
  done: "secondary" as const,
};

export function TaskDetail({
  task,
  terminalSession,
  onStartTerminal,
  onStopTerminal,
  onDeleteTask,
}: TaskDetailProps) {
  if (!task) {
    return (
      <div className="flex h-full items-center justify-center">
        <Empty
          icon={<ListTodo className="h-5 w-5" />}
          title="No task selected"
          description="Select a task from the sidebar"
        />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-border px-4 py-2.5">
        <div className="flex items-center gap-2.5 min-w-0">
          <span className="truncate text-sm font-medium text-fg">{task.title}</span>
          <Badge variant={statusBadgeVariant[task.status]}>{task.status}</Badge>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          {terminalSession ? (
            <Button variant="ghost" size="sm" onClick={onStopTerminal}>
              <Square className="mr-1 h-3 w-3" /> Stop
            </Button>
          ) : (
            <Button variant="secondary" size="sm" onClick={() => onStartTerminal(task.id)}>
              <Terminal className="mr-1 h-3 w-3" /> Terminal
            </Button>
          )}
          <Separator orientation="vertical" className="mx-1 h-5" />
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-fg-secondary hover:text-destructive"
            onClick={() => onDeleteTask(task.id)}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      <div className="flex-1 overflow-auto p-4">
        <div className="space-y-3 text-xs text-fg-secondary">
          <div className="flex gap-2">
            <span className="w-14 shrink-0">ID</span>
            <span className="font-mono text-fg">{task.id}</span>
          </div>
          <div className="flex gap-2">
            <span className="w-14 shrink-0">Project</span>
            <span className="font-mono text-fg">{task.projectId}</span>
          </div>
          <div className="flex gap-2">
            <span className="w-14 shrink-0">Status</span>
            <span className="text-fg">{task.status}</span>
          </div>
          {terminalSession && (
            <>
              <Separator />
              <div className="flex gap-2">
                <span className="w-14 shrink-0">Session</span>
                <span className="font-mono text-fg">{terminalSession.id}</span>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
