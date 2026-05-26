import { useState, useEffect, useCallback } from "react";
import { useContractsClient } from "../context/contracts-context";
import type { Task, TaskStatus } from "@ora/contracts";

export function useTasks(projectId: string | null) {
  const client = useContractsClient();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const res = await client.listTasks({});
      setTasks(res.tasks);
    } finally {
      setLoading(false);
    }
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const createTask = useCallback(
    async (title: string) => {
      if (!projectId) return;
      const res = await client.createTask({
        projectId,
        title,
        status: "todo" as TaskStatus,
      });
      setTasks((prev) => [res.task, ...prev]);
      return res.task;
    },
    [client, projectId],
  );

  const deleteTask = useCallback(
    async (taskId: string) => {
      await client.deleteTask({ taskId });
      setTasks((prev) => prev.filter((t) => t.id !== taskId));
    },
    [client],
  );

  return { tasks, loading, createTask, deleteTask, refresh };
}
