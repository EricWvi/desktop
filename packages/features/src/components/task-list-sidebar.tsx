import type { Task } from "@ora/contracts";
import { Button, Badge, ScrollArea, Empty, ThemeToggle } from "@ora/ui";
import { Plus, Trash2, CheckCircle2, Circle, Clock } from "lucide-react";
import { cn } from "@ora/ui";

interface TaskListSidebarProps {
  tasks: Task[];
  selectedTaskId: string | null;
  onSelectTask: (taskId: string) => void;
  onCreateClick: () => void;
  onDeleteTask: (taskId: string) => void;
}

const statusIcon = {
  todo: <Circle className="h-3 w-3" />,
  doing: <Clock className="h-3 w-3" />,
  done: <CheckCircle2 className="h-3 w-3" />,
};

const statusBadgeVariant = {
  todo: "outline" as const,
  doing: "default" as const,
  done: "secondary" as const,
};

export function TaskListSidebar({
  tasks,
  selectedTaskId,
  onSelectTask,
  onCreateClick,
  onDeleteTask,
}: TaskListSidebarProps) {
  return (
    <div className="flex h-full flex-col bg-bg-secondary">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-xs font-medium text-fg-secondary uppercase tracking-wider">Tasks</span>
        <div className="flex items-center gap-1">
          <ThemeToggle />
          <Button variant="ghost" size="icon" onClick={onCreateClick}>
            <Plus className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {tasks.length === 0 ? (
        <Empty
          icon={<CheckCircle2 className="h-5 w-5" />}
          title="No tasks yet"
          description="Create a task to get started"
          action={
            <Button variant="ghost" size="sm" onClick={onCreateClick}>
              <Plus className="mr-1 h-3 w-3" /> New Task
            </Button>
          }
        />
      ) : (
        <ScrollArea className="flex-1">
          <div className="flex flex-col gap-0.5 p-1.5">
            {tasks.map((task) => (
              <div
                key={task.id}
                onClick={() => onSelectTask(task.id)}
                className={cn(
                  "group flex items-center gap-2 rounded-md px-2.5 py-1.5 cursor-pointer text-sm transition-colors",
                  selectedTaskId === task.id
                    ? "bg-primary-transparent text-fg"
                    : "text-fg-secondary hover:bg-bg-subtle hover:text-fg",
                )}
              >
                <span className="text-fg-secondary">{statusIcon[task.status]}</span>
                <span className="flex-1 truncate">{task.title}</span>
                <Badge variant={statusBadgeVariant[task.status]} className="text-[10px] px-1.5 py-0">
                  {task.status}
                </Badge>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-5 w-5 opacity-0 group-hover:opacity-100 shrink-0"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteTask(task.id);
                  }}
                >
                  <Trash2 className="h-3 w-3" />
                </Button>
              </div>
            ))}
          </div>
        </ScrollArea>
      )}
    </div>
  );
}
