import { useState, useEffect, useCallback } from "react";
import { useContractsClient } from "../context/contracts-context";
import type { Session } from "@ora/contracts";

export function useSessions() {
  const client = useContractsClient();
  const [sessions, setSessions] = useState<Session[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const res = await client.listSessions({});
      setSessions(res.sessions);
    } finally {
      setLoading(false);
    }
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const createTerminalSession = useCallback(
    async (taskId: string) => {
      const res = await client.createSession({
        taskId,
        agentId: "terminal",
        agentSessionId: null,
        status: "running",
        terminal: { cols: 80, rows: 24 },
      });
      setSessions((prev) => [...prev, res.session]);
      return res.session;
    },
    [client],
  );

  const getTerminalSession = useCallback(
    (taskId: string) => sessions.find((s) => s.taskId === taskId && s.status === "running"),
    [sessions],
  );

  return { sessions, loading, createTerminalSession, getTerminalSession, refresh };
}
