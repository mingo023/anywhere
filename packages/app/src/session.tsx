import React, { createContext, useCallback, useContext, useMemo, useRef, useState } from "react";
import type {
  AgentSummary,
  PermissionRequest,
  ProfileSummary,
  ServerMessage,
  SessionTask,
  TimelineItem,
} from "@pocket/protocol";
import { PocketClient, type ConnectionState } from "./client";

type Timelines = Record<string, readonly TimelineItem[]>;
type Tasks = Record<string, readonly SessionTask[]>;

function mergeItem(items: readonly TimelineItem[], item: TimelineItem): readonly TimelineItem[] {
  const at = items.findIndex((existing) => existing.id === item.id);
  if (at >= 0) {
    const next = items.slice();
    next[at] = item;
    return next;
  }
  return [...items, item];
}

type Session = {
  state: ConnectionState;
  agents: readonly AgentSummary[];
  profiles: readonly ProfileSummary[];
  timelines: Timelines;
  tasks: Tasks;
  permission?: PermissionRequest;
  error?: string;
  connect: (host: string, token: string) => void;
  disconnect: () => void;
  createAgent: (cwd: string, prompt: string, profileId: string) => void;
  prompt: (agentId: string, text: string) => void;
  compact: (agentId: string) => void;
  interrupt: (agentId: string) => void;
  stopTask: (agentId: string, taskId: string) => void;
  loadTimeline: (agentId: string) => void;
  resolvePermission: (requestId: string, decision: "allow" | "deny") => void;
  clearError: () => void;
};

const SessionContext = createContext<Session | null>(null);

export function SessionProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<ConnectionState>("idle");
  const [agents, setAgents] = useState<readonly AgentSummary[]>([]);
  const [profiles, setProfiles] = useState<readonly ProfileSummary[]>([]);
  const [timelines, setTimelines] = useState<Timelines>({});
  const [tasks, setTasks] = useState<Tasks>({});
  const [permission, setPermission] = useState<PermissionRequest>();
  const [error, setError] = useState<string>();
  const clientRef = useRef<PocketClient>(null);

  const onMessage = useCallback((msg: ServerMessage) => {
    switch (msg.type) {
      case "hello.ok":
        clientRef.current?.send({ type: "profile.list" });
        break;
      case "profile.list":
        setProfiles(msg.profiles);
        break;
      case "agent.list":
        setAgents(msg.agents);
        break;
      case "agent.update":
        setAgents((prev) => {
          const at = prev.findIndex((a) => a.id === msg.agent.id);
          if (at < 0) return [msg.agent, ...prev];
          const next = prev.slice();
          next[at] = msg.agent;
          return next;
        });
        break;
      case "agent.stream":
        setTimelines((prev) => ({ ...prev, [msg.agentId]: mergeItem(prev[msg.agentId] ?? [], msg.item) }));
        break;
      case "agent.timeline":
        setTimelines((prev) => ({ ...prev, [msg.agentId]: msg.items }));
        break;
      case "agent.tasks":
        setTasks((prev) => ({ ...prev, [msg.agentId]: msg.tasks }));
        break;
      case "permission.request":
        setPermission(msg.request);
        break;
      case "permission.resolved":
        setPermission((prev) => (prev?.requestId === msg.requestId ? undefined : prev));
        break;
      case "error":
        setError(msg.message);
        break;
    }
  }, []);

  const connect = useCallback(
    (host: string, token: string) => {
      clientRef.current?.close();
      const client = new PocketClient(`ws://${host}`, token, "pocket-app", onMessage, setState);
      clientRef.current = client;
      client.connect();
    },
    [onMessage],
  );

  const value = useMemo<Session>(
    () => ({
      state,
      agents,
      profiles,
      timelines,
      tasks,
      permission,
      error,
      connect,
      disconnect: () => {
        clientRef.current?.close();
        clientRef.current = null;
        setAgents([]);
        setProfiles([]);
        setTimelines({});
        setTasks({});
        setError(undefined);
      },
      createAgent: (cwd, prompt, profileId) =>
        clientRef.current?.send({ type: "agent.create", cwd, prompt, profileId, permissionMode: "default" }),
      prompt: (agentId, text) => {
        setError(undefined);
        clientRef.current?.send({ type: "agent.prompt", agentId, text });
      },
      compact: (agentId) => clientRef.current?.send({ type: "agent.compact", agentId }),
      interrupt: (agentId) => clientRef.current?.send({ type: "agent.interrupt", agentId }),
      stopTask: (agentId, taskId) => clientRef.current?.send({ type: "task.stop", agentId, taskId }),
      loadTimeline: (agentId) => clientRef.current?.send({ type: "agent.timeline", agentId }),
      resolvePermission: (requestId, decision) => {
        setPermission(undefined);
        clientRef.current?.send({ type: "permission.resolve", requestId, decision });
      },
      clearError: () => setError(undefined),
    }),
    [state, agents, profiles, timelines, tasks, permission, error, connect],
  );

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): Session {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession outside SessionProvider");
  return ctx;
}
