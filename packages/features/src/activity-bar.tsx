import React from "react";
import type { Session } from "@ora/contracts";
import {
  Button,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
  cn,
} from "@ora/ui";
import { Terminal } from "lucide-react";

interface ActivityBarProps {
  sessions: Session[];
  visibleSessionId: string | null;
  onToggle: (sessionId: string) => void;
}

export function ActivityBar({
  sessions,
  visibleSessionId,
  onToggle,
}: ActivityBarProps) {
  return (
    <TooltipProvider delayDuration={400}>
      <aside className="w-10 flex flex-col border-l bg-muted/40 shrink-0 items-center pt-2 gap-1">
        {sessions.map((session, index) => (
          <Tooltip key={session.id}>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => onToggle(session.id)}
                className={cn(
                  visibleSessionId === session.id && "bg-accent text-accent-foreground",
                )}
              >
                <Terminal size={15} />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="left">Terminal {index + 1}</TooltipContent>
          </Tooltip>
        ))}
      </aside>
    </TooltipProvider>
  );
}
