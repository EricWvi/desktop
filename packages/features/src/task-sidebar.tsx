import React from "react";
import type { Task } from "@ora/contracts";
import { Button, ScrollArea, cn } from "@ora/ui";
import { Plus, Circle, CircleDot, CircleCheck } from "lucide-react";

interface TaskSidebarProps {
  tasks: Task[];
  selectedTaskId: string | null;
  onSelect: (id: string) => void;
  onNewTask?: () => void;
}

const STATUS_ICON: Record<Task["status"], React.ElementType> = {
  todo: Circle,
  doing: CircleDot,
  done: CircleCheck,
};

export function TaskSidebar({
  tasks,
  selectedTaskId,
  onSelect,
  onNewTask,
}: TaskSidebarProps) {
  return (
    <aside className="w-60 flex flex-col border-r bg-muted/40 shrink-0">
      <div className="h-10 flex items-center justify-between px-3 border-b">
        <span className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
          Tasks
        </span>
        <Button variant="ghost" size="icon" onClick={onNewTask} title="New task">
          <Plus size={14} />
        </Button>
      </div>
      <ScrollArea className="flex-1">
        <div className="p-1.5 flex flex-col gap-0.5">
          {tasks.map((task) => {
            const Icon = STATUS_ICON[task.status];
            const isSelected = task.id === selectedTaskId;
            return (
              <button
                key={task.id}
                onClick={() => onSelect(task.id)}
                className={cn(
                  "w-full flex items-center gap-2 px-2.5 py-1.5 rounded-sm text-left text-[13px] transition-colors cursor-pointer",
                  isSelected
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                )}
              >
                <Icon size={13} className="shrink-0 opacity-70" />
                <span className="truncate">{task.title}</span>
              </button>
            );
          })}
        </div>
      </ScrollArea>
    </aside>
  );
}
