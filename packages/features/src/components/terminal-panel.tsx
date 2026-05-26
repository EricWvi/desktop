import { useTerminal } from "../hooks/use-terminal";
import { Spinner } from "@ora/ui";
import { AlertTriangle } from "lucide-react";

interface TerminalPanelProps {
  sessionId: string | null;
  visible: boolean;
}

export function TerminalPanel({ sessionId, visible }: TerminalPanelProps) {
  const { containerRef, status, error } = useTerminal(sessionId, visible);

  return (
    <div className="relative flex h-full flex-col bg-[#1e1e2e]">
      <div className="flex items-center gap-2 border-b border-border px-3 py-1">
        <span className="text-xs font-medium text-fg-secondary">Terminal</span>
        {status === "connecting" && <Spinner className="h-3 w-3" />}
        {status === "connected" && (
          <span className="h-1.5 w-1.5 rounded-full bg-green-500" />
        )}
        {status === "exited" && (
          <span className="text-xs text-fg-secondary">exited</span>
        )}
      </div>

      <div ref={containerRef} className="flex-1 overflow-hidden" />

      {status === "error" && error && (
        <div className="absolute inset-x-0 bottom-0 flex items-center gap-2 bg-red-900/80 px-3 py-2 text-xs text-red-200">
          <AlertTriangle className="h-3 w-3 shrink-0" />
          <span className="flex-1">{error}</span>
        </div>
      )}
    </div>
  );
}
