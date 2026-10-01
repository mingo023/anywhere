import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { AppState, Platform } from "react-native";
import NetInfo from "@react-native-community/netinfo";
import type { AgentSummary, LaunchSpec, Project, ProviderInfo, ServerMessage, TimelineItem } from "@pocket/protocol";
import { PocketClient, type ConnectionState } from "./client";
import { applyAgentUpdate } from "./agents";
import type { Outcome } from "./connection";
import { banner, noLink, retry as retryLink, shouldRedial, track, type Banner, type Link, type Net } from "./connectivity";
import { clientId, load, save, serially, wipe, type Creds } from "./credentials";
import { createDrafts, type Drafts } from "./drafts";
import { keychain } from "./keychain";
import { reconnect, type Launch } from "./launch";
import { pair, wsURL, type PairLink } from "./pairing";
import { STALE, acked, add, empty, failed, resolved, sent, type Permissions } from "./permissions";

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

export type Ended = Exclude<Outcome, "retry">;
export type Providers = { list: readonly ProviderInfo[]; phoneMaxAccess: string };

type Session = {
  link: Link;
  host?: string;
  creds?: Creds | null;
  unreadable: boolean;
  ended?: Ended;
  agents: readonly AgentSummary[];
  timelines: Timelines;
  hasOlder: Readonly<Record<string, boolean>>;
  permissions: Permissions;
  error?: string;
  drafts: Drafts;
  caps: readonly string[];
  projects?: readonly Project[];
  providers?: Providers;
  launch?: Launch;
  openLaunch: () => void;
  create: (spec: LaunchSpec) => string | undefined;
  pairWith: (link: PairLink, name: string) => Promise<void>;
  retry: () => void;
  redial: () => void;
  pairAgain: () => void;
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
  const [link, setLink] = useState<Link>(noLink);
  const [host, setHost] = useState<string>();
  const [agents, setAgents] = useState<readonly AgentSummary[]>([]);
  const [timelines, setTimelines] = useState<Timelines>({});
  const [hasOlder, setHasOlder] = useState<Record<string, boolean>>({});
  const [permissions, setPermissions] = useState<Permissions>(empty);
  const [error, setError] = useState<string>();
  const [drafts] = useState(createDrafts);
  const [creds, setCreds] = useState<Creds | null>();
  const [unreadable, setUnreadable] = useState(false);
  const [vault] = useState(serially);
  const [ended, setEnded] = useState<Ended>();
  const [caps, setCaps] = useState<readonly string[]>([]);
  const [projects, setProjects] = useState<readonly Project[]>();
  const [providers, setProviders] = useState<Providers>();
  const [launch, setLaunch] = useState<Launch>();
  const launchRef = useRef<Launch | undefined>(undefined);
  const clientRef = useRef<PocketClient>(null);
  const stateRef = useRef<ConnectionState>("idle");
  /** The last ids passed to view(), kept even when the send was dropped, so a reconnect can restore them. */
  const viewing = useRef<readonly string[]>([]);
  const resync = useRef(false);

  const onState = useCallback((state: ConnectionState) => {
    stateRef.current = state;
    setLink((prev) => track(prev, state, Date.now()));
  }, []);

  const settle = useCallback((next: Launch | undefined) => {
    launchRef.current = next;
    setLaunch(next);
  }, []);

  const onMessage = useCallback((msg: ServerMessage) => {
    switch (msg.type) {
      case "hello.ok": {
        setHost(msg.hostname);
        setPermissions(empty);
        setCaps(msg.caps ?? []);
        resync.current = true;
        const next = reconnect(launchRef.current, Date.now());
        const client = clientRef.current;
        if (next?.send && client) settle({ ...next.launch, id: client.send(next.send) });
        else if (next && !next.send) settle(next.launch);
        break;
      }
      case "project.list":
        setProjects(msg.projects);
        break;
      case "agent.providers":
        setProviders({ list: msg.providers, phoneMaxAccess: msg.phoneMaxAccess });
        break;
      case "agent.created":
        if (launchRef.current?.requestId === msg.requestId) settle({ ...launchRef.current, agentId: msg.agentId });
        break;
      case "agent.list": {
        setAgents(msg.agents);
        if (!resync.current) break;
        resync.current = false;
        const ids = viewing.current.filter((id) => msg.agents.some((a) => a.id === id));
        if (!ids.length) break;
        clientRef.current?.send({ type: "agent.view", agentIds: ids });
        for (const agentId of ids) clientRef.current?.send({ type: "agent.timeline", agentId });
        break;
      }
      case "agent.update":
        if (msg.agent.status === "closed") drafts.clear(msg.agent.id);
        setAgents((prev) => applyAgentUpdate(prev, msg.agent));
        break;
      case "agent.stream":
        setTimelines((prev) => ({ ...prev, [msg.agentId]: mergeItem(prev[msg.agentId] ?? [], msg.item) }));
        break;
      case "agent.timeline":
        setTimelines((prev) => ({ ...prev, [msg.agentId]: msg.items }));
        setHasOlder((prev) => ({ ...prev, [msg.agentId]: msg.hasOlder }));
        break;
      case "permission.request":
        setPermissions((p) => add(p, msg.request));
        break;
      case "permission.resolved":
        setPermissions((p) => resolved(p, msg.requestId));
        break;
      case "ack":
        setPermissions((p) => acked(p, msg.id));
        break;
      case "error": {
        const { id, message } = msg;
        if (id !== undefined && id === launchRef.current?.id) {
          settle({ ...launchRef.current, failure: { code: msg.code, message, detail: msg.detail } });
          break;
        }
        if (id !== undefined) setPermissions((p) => failed(p, id, message));
        if (message !== STALE) setError(message);
        break;
      }
    }
  }, [drafts, settle]);

  const forget = useCallback(() => {
    setAgents([]);
    setTimelines({});
    setHasOlder({});
    setPermissions(empty);
    setError(undefined);
    setHost(undefined);
    setCaps([]);
    setProjects(undefined);
    setProviders(undefined);
    settle(undefined);
  }, [settle]);

  const onEnd = useCallback(
    (why: Ended) => {
      forget();
      setEnded(why);
      if (why === "revoked") {
        setCreds(null);
        // A token the wipe leaves behind is refused again on the next hello.
        vault(() => wipe(keychain)).catch(() => {});
      }
    },
    [forget, vault],
  );

  const start = useCallback(
    async (c: Creds) => {
      const id = await clientId(keychain, Platform.OS);
      clientRef.current?.close();
      setHost(c.host);
      const client = new PocketClient(wsURL(c.host), c.token, id, onMessage, onState, onEnd);
      clientRef.current = client;
      setEnded(undefined);
      client.connect();
    },
    [onMessage, onState, onEnd],
  );

  const resume = useCallback(
    (c: Creds | null) => {
      setCreds(c);
      if (c) start(c).catch(() => setUnreadable(true));
    },
    [start],
  );

  const boot = useCallback(() => {
    setUnreadable(false);
    vault(() => load(keychain)).then(resume, () => setUnreadable(true));
  }, [resume, vault]);

  useEffect(boot, [boot]);

  const pairWith = useCallback(
    async (link: PairLink, name: string) => {
      const platform = Platform.OS === "android" ? "android" : "ios";
      const { deviceId, token } = await pair(link.host, { code: link.code, name, platform });
      const c = { host: link.host, token, deviceId, macName: link.name };
      await vault(() => save(keychain, c));
      forget();
      setUnreadable(false);
      setCreds(c);
      await start(c);
    },
    [forget, start, vault],
  );

  const retry = useCallback(() => {
    if (unreadable) boot();
    else if (creds) resume(creds);
  }, [boot, creds, resume, unreadable]);

  const redial = useCallback(() => {
    setLink((prev) => retryLink(prev, Date.now()));
    clientRef.current?.redial();
  }, []);

  useEffect(() => {
    const change = AppState.addEventListener("change", (state) => {
      if (state === "active" && stateRef.current !== "online") redial();
    });
    return () => change.remove();
  }, [redial]);

  useEffect(() => {
    let prev: Net | undefined;
    return NetInfo.addEventListener((state) => {
      const next = { isConnected: state.isConnected, type: state.type };
      if (shouldRedial(prev, next)) redial();
      prev = next;
    });
  }, [redial]);

  const loadTimeline = useCallback(
    (agentId: string) => clientRef.current?.send({ type: "agent.timeline", agentId }),
    [],
  );

  const view = useCallback((agentIds: readonly string[]) => {
    viewing.current = agentIds;
    clientRef.current?.send({ type: "agent.view", agentIds });
  }, []);

  const openLaunch = useCallback(() => {
    clientRef.current?.send({ type: "project.list" });
    clientRef.current?.send({ type: "agent.providers" });
  }, []);

  const create = useCallback((spec: LaunchSpec) => {
    const client = clientRef.current;
    if (!client) return undefined;
    const sentAt = Date.now();
    const requestId = `${sentAt.toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
    settle({ requestId, spec, sentAt, id: client.send({ type: "agent.create", requestId, spec }) });
    return requestId;
  }, [settle]);

  const value = useMemo<Session>(
    () => ({
      link,
      host,
      agents,
      timelines,
      hasOlder,
      permissions,
      error,
      drafts,
      caps,
      projects,
      providers,
      launch,
      openLaunch,
      create,
      creds,
      unreadable,
      ended,
      pairWith,
      retry,
      redial,
      pairAgain: () => setEnded(undefined),
      disconnect: () => {
        clientRef.current?.close();
        clientRef.current = null;
        forget();
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
        const msgId = clientRef.current?.send({ type: "permission.resolve", requestId, decision, ...answer });
        if (msgId !== undefined) setPermissions((p) => sent(p, msgId, requestId));
      },
      clearError: () => setError(undefined),
    }),
    [link, host, agents, timelines, hasOlder, permissions, error, drafts, caps, projects, providers, launch, openLaunch, create, creds, unreadable, ended, pairWith, retry, redial, forget, loadTimeline, view],
  );

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): Session {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession outside SessionProvider");
  return ctx;
}

/** Ticks once a second while the link is down, so the grace and unreachable thresholds show on time. */
export function useBanner(): Banner | null {
  const { link, host } = useSession();
  const [now, setNow] = useState(Date.now);

  useEffect(() => {
    setNow(Date.now());
    if (link.state === "online") return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [link.state]);

  return banner(link, now, host ?? "");
}
