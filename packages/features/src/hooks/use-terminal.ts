import { useRef, useState, useEffect, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { TerminalServerMessage } from "@ora/contracts";

export function useTerminal(sessionId: string | null, enabled: boolean) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const [status, setStatus] = useState<"connecting" | "connected" | "exited" | "error">("connecting");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!enabled || !sessionId || !containerRef.current) return;

    const term = new Terminal({
      fontSize: 13,
      fontFamily: "var(--font-mono)",
      cursorBlink: true,
      theme: {
        background: "#1e1e2e",
        foreground: "#cdd6f4",
        cursor: "#f5e0dc",
        selectionBackground: "#585b7066",
      },
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(containerRef.current);
    termRef.current = term;

    // Delay first fit until the container has been laid out
    requestAnimationFrame(() => {
      try { fitAddon.fit(); } catch { /* container not ready yet */ }
    });

    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(`${proto}//${location.host}/api/sessions/${sessionId}/terminal`);
    wsRef.current = ws;

    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data) as TerminalServerMessage;
      switch (msg.type) {
        case "ready":
          setStatus("connected");
          break;
        case "history":
          term.write(msg.data);
          break;
        case "output":
          term.write(msg.data);
          break;
        case "exit":
          setStatus("exited");
          break;
        case "error":
          setError(msg.message);
          setStatus("error");
          break;
      }
    };

    term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: "input", data }));
      }
    });

    const resizeObserver = new ResizeObserver(() => {
      try { fitAddon.fit(); } catch { /* ignore when terminal not ready */ }
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: "resize", cols: term.cols, rows: term.rows }));
      }
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      resizeObserver.disconnect();
      term.dispose();
      termRef.current = null;
      ws.close();
      wsRef.current = null;
    };
  }, [sessionId, enabled]);

  const kill = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type: "kill" }));
    }
  }, []);

  return { containerRef, status, error, kill };
}
