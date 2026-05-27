import React, { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import type { TerminalClientMessage, TerminalServerMessage } from "@ora/contracts";

interface TerminalPanelProps {
  sessionId: string;
  isVisible: boolean;
}

// Detect the user's OS so Ctrl+C copy-vs-signal behavior can differ per platform.
const isWindows =
  typeof navigator !== "undefined" &&
  /Win/i.test(navigator.platform || navigator.userAgent);

// How long to wait before the first reconnect attempt.
const RECONNECT_INITIAL_DELAY_MS = 300;
// Reconnect attempts before giving up. Resets whenever "ready" is received.
const RECONNECT_MAX_ATTEMPTS = 6;

// Convert a CSS custom-property value to a concrete color string the browser
// can resolve, by momentarily mounting a throw-away element.
function resolveCssVar(property: string): string {
  const el = document.createElement("div");
  el.style.color = `var(${property})`;
  el.style.position = "fixed";
  el.style.pointerEvents = "none";
  el.style.opacity = "0";
  document.body.appendChild(el);
  const resolved = getComputedStyle(el).color;
  document.body.removeChild(el);
  return resolved || "#000000";
}

function buildXtermTheme() {
  return {
    background: resolveCssVar("--background"),
    foreground: resolveCssVar("--foreground"),
    cursor: resolveCssVar("--foreground"),
    cursorAccent: resolveCssVar("--background"),
    selectionBackground: resolveCssVar("--accent"),
    selectionForeground: resolveCssVar("--accent-foreground"),
    // Standard ANSI palette – VS Code Dark+ inspired, works on both themes.
    black: "#000000",
    red: "#cd3131",
    green: "#0dbc79",
    yellow: "#e5e510",
    blue: "#2472c8",
    magenta: "#bc3fbc",
    cyan: "#11a8cd",
    white: "#e5e5e5",
    brightBlack: "#666666",
    brightRed: "#f14c4c",
    brightGreen: "#23d18b",
    brightYellow: "#f5f543",
    brightBlue: "#3b8eea",
    brightMagenta: "#d670d6",
    brightCyan: "#29b8db",
    brightWhite: "#e5e5e5",
  };
}

// Build the WebSocket URL for a terminal session. Works with Vite's /api proxy
// as well as direct connections.
function buildTerminalWsUrl(sessionId: string): string {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/api/sessions/${sessionId}/terminal`;
}

export function TerminalPanel({ sessionId, isVisible }: TerminalPanelProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  // Always points to the most-recently created WebSocket so that closures
  // (onData, onResize) never hold stale references across reconnects.
  const wsRef = useRef<WebSocket | null>(null);

  // Main lifecycle: create the xterm instance and keep the WebSocket alive.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    // --- Terminal ---
    const term = new Terminal({
      theme: buildXtermTheme(),
      fontFamily:
        'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      scrollback: 10000,
      allowProposedApi: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);
    fitAddon.fit();

    // WebGL renderer – must be loaded after open(). Falls back to Canvas 2D
    // automatically when WebGL is unavailable (e.g. headless environments).
    const webglAddon = new WebglAddon();
    webglAddon.onContextLoss(() => webglAddon.dispose());
    try {
      term.loadAddon(webglAddon);
    } catch {
      // Canvas 2D fallback is already active.
    }

    fitAddonRef.current = fitAddon;

    // Sends a message on the current WebSocket if it is open. Using wsRef
    // inside the closures below means we always target the live socket, even
    // after a reconnect has replaced it.
    function sendToWs(msg: TerminalClientMessage): void {
      const ws = wsRef.current;
      if (ws?.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(msg));
      }
    }

    // --- WebSocket management with reconnection ---
    let stopped = false; // set to true when the effect cleans up
    let attempt = 0;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    function connect(): void {
      if (stopped) return;

      const ws = new WebSocket(buildTerminalWsUrl(sessionId));
      wsRef.current = ws;

      ws.onmessage = (event: MessageEvent) => {
        let msg: TerminalServerMessage;
        try {
          msg = JSON.parse(event.data as string) as TerminalServerMessage;
        } catch {
          return;
        }
        switch (msg.type) {
          case "ready":
            // Server-side PTY is ready; sync the initial terminal dimensions.
            sendToWs({ type: "resize", cols: term.cols, rows: term.rows });
            attempt = 0;
            break;
          case "history":
          case "output":
            term.write(msg.data);
            break;
          case "exit":
            term.writeln(
              `\r\n\x1b[38;5;240m[Process exited${msg.exitCode !== null ? ` with code ${msg.exitCode}` : ""}]\x1b[0m`,
            );
            break;
          case "error":
            term.writeln(`\r\n\x1b[31m[Error: ${msg.message}]\x1b[0m`);
            break;
        }
      };

      ws.onclose = () => {
        if (stopped) return;
        // Exponential backoff: the first retry is fast to recover from the
        // transient "already attached" 409 caused by React StrictMode's
        // double-invocation, while later retries slow down to avoid hammering
        // a genuinely unavailable server.
        if (attempt >= RECONNECT_MAX_ATTEMPTS) return;
        const delay = RECONNECT_INITIAL_DELAY_MS * 2 ** attempt;
        attempt += 1;
        retryTimer = setTimeout(connect, delay);
      };
    }

    // A short delay before the first connect lets React StrictMode's cleanup cancel
    // the timer before any WebSocket is created, eliminating the transient "closed
    // before connection established" warning without needing server-side logic.
    retryTimer = setTimeout(connect, 80);

    // --- Input ---
    // Send user keystrokes to the PTY through the current WebSocket. Because
    // sendToWs reads wsRef.current, Ctrl+C (and all other input) will reach
    // a reconnected socket without needing to re-register this handler.
    const onDataDisposable = term.onData((data) => {
      sendToWs({ type: "input", data });
    });

    // Relay viewport dimension changes to the server-side PTY.
    const onResizeDisposable = term.onResize(({ cols, rows }) => {
      sendToWs({ type: "resize", cols, rows });
    });

    // Windows: Ctrl+C with an active selection copies to clipboard instead of
    // sending SIGINT, matching the behaviour users expect from native terminals.
    term.attachCustomKeyEventHandler((event: KeyboardEvent) => {
      if (event.type !== "keydown") return true;

      if (isWindows && event.ctrlKey && event.key === "c") {
        const selection = term.getSelection();
        if (selection) {
          navigator.clipboard.writeText(selection).catch(() => {
            document.execCommand("copy");
          });
          term.clearSelection();
          return false;
        }
      }

      // Mac: Cmd+C copy is handled by the browser natively.
      return true;
    });

    // Auto-fit when the container element is resized.
    const resizeObserver = new ResizeObserver(() => {
      requestAnimationFrame(() => {
        fitAddon.fit();
      });
    });
    resizeObserver.observe(container);

    // Theme: re-apply whenever the .dark class toggles on <html>.
    const themeObserver = new MutationObserver(() => {
      term.options.theme = buildXtermTheme();
    });
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    return () => {
      stopped = true;
      if (retryTimer !== null) clearTimeout(retryTimer);
      const ws = wsRef.current;
      if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
        ws.close();
      }
      wsRef.current = null;
      onDataDisposable.dispose();
      onResizeDisposable.dispose();
      resizeObserver.disconnect();
      themeObserver.disconnect();
      term.dispose();
      fitAddonRef.current = null;
    };
  // sessionId drives the entire lifecycle; a change tears down the current
  // terminal and WebSocket pair and builds a fresh one.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  // Re-fit whenever the panel becomes visible again after a panel switch.
  useEffect(() => {
    if (isVisible) {
      requestAnimationFrame(() => {
        fitAddonRef.current?.fit();
      });
    }
  }, [isVisible]);

  return (
    <div
      // Keep the panel in the DOM when hidden so the PTY connection and scroll
      // history survive switching between multiple terminals.
      style={{ display: isVisible ? "flex" : "none" }}
      className="flex-1 flex flex-col min-h-0 overflow-hidden"
    >
      {/*
       * Three-layer stack (bottom → top):
       *   1. This div's CSS background  → adapts to the current theme palette.
       *   2. WebGL canvas (xterm)       → GPU-rendered glyphs / colours.
       *   3. DOM text layer (xterm)     → browser-native text selection & IME.
       *
       * xterm creates layers 2 and 3 automatically inside containerRef; we
       * only need to provide the background-colour foundation here.
       */}
      <div
        ref={containerRef}
        className="flex-1 min-h-0 p-1"
        style={{ background: "var(--background)" }}
      />
    </div>
  );
}
