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
// as well as direct connections once the proxy translates http→ws.
function buildTerminalWsUrl(sessionId: string): string {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/api/sessions/${sessionId}/terminal`;
}

export function TerminalPanel({ sessionId, isVisible }: TerminalPanelProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

  // Main lifecycle: create terminal, attach addons, open WebSocket.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const term = new Terminal({
      theme: buildXtermTheme(),
      fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      scrollback: 10000,
      // Allow the terminal to use the full container width/height.
      allowProposedApi: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);

    term.open(container);
    fitAddon.fit();

    // WebGL renderer – must be loaded after open().
    const webglAddon = new WebglAddon();
    // Fall back gracefully if WebGL is unavailable (e.g. headless test env).
    webglAddon.onContextLoss(() => {
      webglAddon.dispose();
    });
    try {
      term.loadAddon(webglAddon);
    } catch {
      // Canvas 2D fallback is already active when WebGL fails to initialise.
    }

    termRef.current = term;
    fitAddonRef.current = fitAddon;

    // --- WebSocket connection ---
    const wsUrl = buildTerminalWsUrl(sessionId);
    const ws = new WebSocket(wsUrl);

    ws.onmessage = (event: MessageEvent) => {
      let msg: TerminalServerMessage;
      try {
        msg = JSON.parse(event.data as string) as TerminalServerMessage;
      } catch {
        return;
      }
      switch (msg.type) {
        case "ready":
          // Server is ready – send the initial terminal dimensions.
          sendWsMessage(ws, {
            type: "resize",
            cols: term.cols,
            rows: term.rows,
          });
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

    // Send user keystrokes to the PTY.
    const onDataDisposable = term.onData((data) => {
      sendWsMessage(ws, { type: "input", data });
    });

    // Relay dimension changes so the server-side PTY stays in sync.
    const onResizeDisposable = term.onResize(({ cols, rows }) => {
      if (ws.readyState === WebSocket.OPEN) {
        sendWsMessage(ws, { type: "resize", cols, rows });
      }
    });

    // Windows: Ctrl+C with an active selection copies instead of sending SIGINT.
    term.attachCustomKeyEventHandler((event: KeyboardEvent) => {
      if (event.type !== "keydown") return true;

      if (isWindows && event.ctrlKey && event.key === "c") {
        const selection = term.getSelection();
        if (selection) {
          navigator.clipboard.writeText(selection).catch(() => {
            // Fallback: use document.execCommand for older browsers.
            document.execCommand("copy");
          });
          term.clearSelection();
          return false;
        }
      }

      // Mac: Cmd+C is handled by the browser's native clipboard machinery.
      return true;
    });

    // Auto-fit when the container size changes.
    const resizeObserver = new ResizeObserver(() => {
      // Defer to let the browser finish painting the new dimensions.
      requestAnimationFrame(() => {
        fitAddon.fit();
      });
    });
    resizeObserver.observe(container);

    // Theme: re-apply whenever the .dark class is toggled on <html>.
    const themeObserver = new MutationObserver(() => {
      term.options.theme = buildXtermTheme();
    });
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    return () => {
      onDataDisposable.dispose();
      onResizeDisposable.dispose();
      resizeObserver.disconnect();
      themeObserver.disconnect();
      if (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING) {
        ws.close();
      }
      term.dispose();
      termRef.current = null;
      fitAddonRef.current = null;
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  // Re-fit whenever visibility is restored so the canvas matches the container.
  useEffect(() => {
    if (isVisible) {
      requestAnimationFrame(() => {
        fitAddonRef.current?.fit();
      });
    }
  }, [isVisible]);

  return (
    <div
      // Outer wrapper controls visibility without unmounting – preserving the
      // terminal session and scroll history across panel switches.
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
        style={{
          // Match xterm's background so there is no colour mismatch during
          // canvas resize or when the terminal has fewer rows than the panel.
          background: "var(--background)",
        }}
      />
    </div>
  );
}

function sendWsMessage(ws: WebSocket, msg: TerminalClientMessage): void {
  if (ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(msg));
  }
}
