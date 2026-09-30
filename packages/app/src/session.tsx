import React, { createContext, useCallback, useContext, useMemo, useRef, useState } from "react";
import type { AgentSummary, PermissionRequest, ServerMessage, TimelineItem } from "@pocket/protocol";
import { PocketClient, type ConnectionState } from "./client";
import { applyAgentUpdate } from "./agents";

type Timelines = Record<string, readonly TimelineItem[]>;

function mergeItem(items: readonly TimelineItem[], item: TimelineItem): readonly TimelineItem[] {
  const at = items.findIndex((existing) => existing.id === item.id);
  if (at >= 0) {
    const next = items.slice();
    next[at] = item;
    return next;
  }
  return [...items, item];
}

export type PermissionAnswer = { option?: string; message?: string };

type Session = {
  state: ConnectionState;
  agents: readonly AgentSummary[];
  timelines: Timelines;
  permission?: PermissionRequest;
  error?: string;
  connect: (host: string, token: string) => void;
  disconnect: () => void;
  prompt: (agentId: string, text: string) => void;
  compact: (agentId: string) => void;
  interrupt: (agentId: string) => void;
  loadTimeline: (agentId: string) => void;
  view: (agentIds: readonly string[]) => void;
  resolvePermission: (requestId: string, decision: "allow" | "deny", answer?: PermissionAnswer) => void;
  clearError: () => void;
};

const SessionContext = createContext<Session | null>(null);

export function SessionProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<ConnectionState>("idle");
  const [agents, setAgents] = useState<readonly AgentSummary[]>([]);
  const [timelines, setTimelines] = useState<Timelines>({});
  const [permission, setPermission] = useState<PermissionRequest>();
  const [error, setError] = useState<string>();
  const clientRef = useRef<PocketClient>(null);

  const onMessage = useCallback((msg: ServerMessage) => {
    switch (msg.type) {
      case "agent.list":
        setAgents(msg.agents);
        break;
      case "agent.update":
        setAgents((prev) => applyAgentUpdate(prev, msg.agent));
        break;
      case "agent.stream":
        setTimelines((prev) => ({ ...prev, [msg.agentId]: mergeItem(prev[msg.agentId] ?? [], msg.item) }));
        break;
      case "agent.timeline":
        setTimelines((prev) => ({ ...prev, [msg.agentId]: msg.items }));
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

  const loadTimeline = useCallback(
    (agentId: string) => clientRef.current?.send({ type: "agent.timeline", agentId }),
    [],
  );

  const view = useCallback(
    (agentIds: readonly string[]) => clientRef.current?.send({ type: "agent.view", agentIds }),
    [],
  );

  const value = useMemo<Session>(
    () => ({
      state,
      agents,
      timelines,
      permission,
      error,
      connect,
      disconnect: () => {
        clientRef.current?.close();
        clientRef.current = null;
        setAgents([]);
        setTimelines({});
        setError(undefined);
      },
      prompt: (agentId, text) => {
        setError(undefined);
        clientRef.current?.send({ type: "agent.prompt", agentId, text });
      },
      compact: (agentId) => clientRef.current?.send({ type: "agent.compact", agentId }),
      interrupt: (agentId) => clientRef.current?.send({ type: "agent.interrupt", agentId }),
      loadTimeline,
      view,
      resolvePermission: (requestId, decision, answer) => {
        setPermission(undefined);
        clientRef.current?.send({ type: "permission.resolve", requestId, decision, ...answer });
      },
      clearError: () => setError(undefined),
    }),
    [state, agents, timelines, permission, error, connect, loadTimeline, view],
  );

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): Session {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession outside SessionProvider");
  return ctx;
}
