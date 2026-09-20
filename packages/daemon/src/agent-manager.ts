import { randomUUID } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { AgentSummary, type PermissionMode, type SessionTask, type TimelineItem } from "@pocket/protocol";
import { Context, Effect, Exit, Layer, PubSub, Ref, Schema, Scope, Stream } from "effect";
import { Broadcast } from "./broadcast.js";
import { AppConfig, POCKET_HOME, type Profile } from "./config.js";
import { UnknownAgent, UnknownProfile, UnknownProvider, type ProviderFailure } from "./errors.js";
import { Permissions } from "./permission-broker.js";
import { claudeProvider } from "./providers/claude.js";
import type { Provider, ProviderEvent, ProviderSession } from "./providers/types.js";
import * as Tl from "./timeline.js";
import { toolDetail } from "./tool-detail.js";

const AGENTS_DIR = join(POCKET_HOME, "agents");
const PROVIDERS = new Map<string, Provider>([[claudeProvider.id, claudeProvider]]);

type ManagedAgent = {
  readonly summary: AgentSummary;
  readonly timeline: Tl.Timeline;
  readonly session: ProviderSession | null;
  readonly scope: Scope.CloseableScope | null;
  /** A /compact turn in flight: "confirmed" once its boundary lands. */
  readonly compaction: "none" | "pending" | "confirmed";
  readonly tasks: ReadonlyMap<string, SessionTask>;
  /** A provider turn is streaming; background tasks outlive it, so both gate the idle status. */
  readonly turn: boolean;
};

type TaskEvent = Extract<ProviderEvent, { kind: "task_start" | "task_progress" | "task_end" | "task_list" }>;

const TASK_KINDS: Record<string, SessionTask["kind"]> = {
  local_agent: "agent",
  remote_agent: "agent",
  local_bash: "shell",
  local_workflow: "workflow",
};

/** A shell's command line rides the tool call that spawned the task, never the task event itself. */
function taskTitle(agent: ManagedAgent, event: Extract<ProviderEvent, { kind: "task_start" }>, kind: SessionTask["kind"]): string {
  if (kind === "shell" && event.toolUseId !== undefined) {
    const index = agent.timeline.toolIndex.get(event.toolUseId);
    const item = index === undefined ? undefined : agent.timeline.items[index];
    if (item?.kind === "tool" && item.call.detail.kind === "shell" && item.call.detail.command) {
      return item.call.detail.command;
    }
  }
  if (kind === "agent" && event.subagentType) return `${event.subagentType} · ${event.description}`;
  return event.description;
}

function reduceTasks(agent: ManagedAgent, event: TaskEvent, ts: number): ReadonlyMap<string, SessionTask> {
  const tasks = new Map(agent.tasks);

  switch (event.kind) {
    case "task_start": {
      const kind = TASK_KINDS[event.taskType];
      if (!kind) return agent.tasks;
      tasks.set(event.taskId, {
        taskId: event.taskId,
        kind,
        title: taskTitle(agent, event, kind),
        status: "running",
        startedAt: ts,
      });
      return tasks;
    }

    case "task_progress": {
      const current = tasks.get(event.taskId);
      if (!current) return agent.tasks;
      const detail = event.summary ?? event.lastTool;
      tasks.set(event.taskId, {
        ...current,
        tools: event.tools,
        tokens: event.tokens,
        ...(detail ? { detail } : {}),
      });
      return tasks;
    }

    case "task_end": {
      const current = tasks.get(event.taskId);
      if (!current || current.status !== "running") return agent.tasks;
      tasks.set(event.taskId, {
        ...current,
        status: event.status,
        endedAt: ts,
        ...(event.summary ? { detail: event.summary } : {}),
      });
      return tasks;
    }

    case "task_list": {
      const live = new Set(event.taskIds);
      let reaped = false;
      for (const [taskId, task] of tasks) {
        if (task.status !== "running" || live.has(taskId)) continue;
        tasks.set(taskId, { ...task, status: "ok", endedAt: ts });
        reaped = true;
      }
      return reaped ? tasks : agent.tasks;
    }
  }
}

export type CreateInput = {
  readonly cwd: string;
  readonly profileId: string;
  readonly prompt: string;
  readonly model?: string | undefined;
  readonly permissionMode?: PermissionMode | undefined;
};

export type CreateError = UnknownProfile | UnknownProvider | ProviderFailure | UnknownAgent;

export interface AgentsService {
  readonly list: Effect.Effect<ReadonlyArray<AgentSummary>>;
  readonly timelineOf: (agentId: string) => Effect.Effect<Tl.Timeline, UnknownAgent>;
  readonly create: (input: CreateInput) => Effect.Effect<AgentSummary, CreateError>;
  readonly prompt: (agentId: string, text: string) => Effect.Effect<void, CreateError>;
  readonly compact: (agentId: string) => Effect.Effect<void, CreateError>;
  readonly tasks: Effect.Effect<ReadonlyArray<{ readonly agentId: string; readonly tasks: ReadonlyArray<SessionTask> }>>;
  readonly stopTask: (agentId: string, taskId: string) => Effect.Effect<void, UnknownAgent>;
  readonly interrupt: (agentId: string) => Effect.Effect<void, UnknownAgent>;
  readonly close: (agentId: string) => Effect.Effect<void, UnknownAgent>;
}

export class Agents extends Context.Tag("Agents")<Agents, AgentsService>() {}

function titleFrom(prompt: string): string {
  const line = prompt.trim().split("\n", 1)[0] ?? "";
  return line.length > 60 ? `${line.slice(0, 60)}…` : line || "Untitled";
}

const encodeSummary = Schema.encodeSync(AgentSummary);
const decodeSummary = Schema.decodeUnknownEither(AgentSummary);

/** Records outlive the daemon; live sessions do not, so restored agents come back closed and resumable. */
function restoreFromDisk(): ReadonlyMap<string, ManagedAgent> {
  mkdirSync(AGENTS_DIR, { recursive: true });
  const map = new Map<string, ManagedAgent>();
  for (const file of readdirSync(AGENTS_DIR).filter((name) => name.endsWith(".json"))) {
    const parsed = decodeSummary(JSON.parse(readFileSync(join(AGENTS_DIR, file), "utf8")));
    if (parsed._tag === "Left") continue;
    const summary: AgentSummary = { ...parsed.right, status: "closed" };
    map.set(summary.id, {
      summary,
      timeline: Tl.empty,
      session: null,
      scope: null,
      compaction: "none",
      tasks: new Map(),
      turn: false,
    });
  }
  return map;
}

const make = Effect.gen(function* () {
  const config = yield* AppConfig;
  const hub = yield* Broadcast;
  const perms = yield* Permissions;
  const agents = yield* Ref.make(yield* Effect.sync(restoreFromDisk));

  const persist = (summary: AgentSummary) =>
    Effect.sync(() =>
      writeFileSync(join(AGENTS_DIR, `${summary.id}.json`), JSON.stringify(encodeSummary(summary), null, 2)),
    ).pipe(Effect.ignore);

  const require_ = (agentId: string): Effect.Effect<ManagedAgent, UnknownAgent> =>
    Effect.flatMap(Ref.get(agents), (map) => {
      const agent = map.get(agentId);
      return agent ? Effect.succeed(agent) : Effect.fail(new UnknownAgent({ agentId }));
    });

  const patch = (agentId: string, f: (agent: ManagedAgent) => ManagedAgent) =>
    Ref.update(agents, (map) => {
      const agent = map.get(agentId);
      return agent ? new Map(map).set(agentId, f(agent)) : map;
    });

  const setStatus = (agentId: string, status: AgentSummary["status"]) =>
    Effect.gen(function* () {
      const updated = yield* Ref.modify(agents, (map) => {
        const agent = map.get(agentId);
        if (!agent) return [null, map] as const;
        const summary: AgentSummary = { ...agent.summary, status, updatedAt: Date.now() };
        return [summary, new Map(map).set(agentId, { ...agent, summary })] as const;
      });
      if (!updated) return;
      yield* persist(updated);
      yield* PubSub.publish(hub, { type: "agent.update", agent: updated });
    });

  const emit = (agentId: string, item: TimelineItem, epoch: number) =>
    PubSub.publish(hub, { type: "agent.stream", agentId, epoch, item });

  const publishTasks = (agentId: string) =>
    Effect.gen(function* () {
      const agent = (yield* Ref.get(agents)).get(agentId);
      if (!agent) return;
      yield* PubSub.publish(hub, { type: "agent.tasks", agentId, tasks: [...agent.tasks.values()] });
    });

  /** Subagents and background shells keep running after the turn's result, so both have to be quiet to be idle. */
  const settle = (agentId: string) =>
    Effect.gen(function* () {
      const agent = (yield* Ref.get(agents)).get(agentId);
      if (!agent) return;
      const { status } = agent.summary;
      if (status !== "running" && status !== "idle") return;
      const busy = agent.turn || [...agent.tasks.values()].some((task) => task.status === "running");
      const next = busy ? "running" : "idle";
      if (status !== next) yield* setStatus(agentId, next);
    });

  const applyEvent = (agentId: string, event: ProviderEvent): Effect.Effect<void> =>
    Effect.gen(function* () {
      if (event.kind === "session") {
        yield* patch(agentId, (agent) => ({
          ...agent,
          summary: {
            ...agent.summary,
            providerSessionId: event.providerSessionId,
            model: event.model,
          },
        }));
        const map = yield* Ref.get(agents);
        /** The SDK re-announces the session on every prompt, so only the initial spawn may clear the status. */
        if (map.get(agentId)?.summary.status === "initializing") yield* setStatus(agentId, "idle");
        return;
      }

      const compaction = yield* Effect.map(Ref.get(agents), (map) => map.get(agentId)?.compaction ?? "none");
      if (compaction !== "none") {
        /** The summarizer streams a whole turn of its own; only its outcome belongs to the conversation. */
        if (event.kind === "compacted") {
          yield* patch(agentId, (agent) => ({ ...agent, compaction: "confirmed" }));
        } else if (event.kind === "result") {
          yield* patch(agentId, (agent) => ({ ...agent, compaction: "none", turn: false }));
          /** The turn's usage is the summarizer's, not the rebuilt context, so the boundary row stands alone. */
          if (compaction === "confirmed") return yield* setStatus(agentId, "idle");
          return yield* applyEvent(agentId, { kind: "error", message: "Compaction did not complete" });
        } else if (event.kind !== "error") {
          return;
        }
      }

      const id = yield* Effect.sync(() => randomUUID());
      const ts = yield* Effect.sync(() => Date.now());

      if (event.kind === "task_start" || event.kind === "task_progress" || event.kind === "task_end" || event.kind === "task_list") {
        yield* patch(agentId, (agent) => ({ ...agent, tasks: reduceTasks(agent, event, ts) }));
        yield* publishTasks(agentId);
        if (event.kind !== "task_progress") yield* settle(agentId);
        return;
      }

      const emitted = yield* Ref.modify(agents, (map) => {
        const agent = map.get(agentId);
        if (!agent) return [null, map] as const;
        const [timeline, item] = Tl.apply(agent.timeline, event, id, ts);
        if (!item) return [null, new Map(map).set(agentId, { ...agent, timeline })] as const;
        const summary: AgentSummary = {
          ...agent.summary,
          maxSeq: timeline.seq,
          epoch: timeline.epoch,
          updatedAt: ts,
        };
        return [{ item, epoch: timeline.epoch }, new Map(map).set(agentId, { ...agent, timeline, summary })] as const;
      });

      if (emitted) yield* emit(agentId, emitted.item, emitted.epoch);
      if (event.kind === "result") {
        yield* patch(agentId, (agent) => ({ ...agent, turn: false }));
        yield* settle(agentId);
      }
      if (event.kind === "error") yield* setStatus(agentId, "error");
    });

  const spawn = (
    agentId: string,
    profile: Profile,
    cwd: string,
    model: string | undefined,
    permissionMode: PermissionMode | undefined,
    resume: string | undefined,
  ) =>
    Effect.gen(function* () {
      const provider = PROVIDERS.get(profile.provider);
      if (!provider) return yield* new UnknownProvider({ provider: profile.provider });

      const scope = yield* Scope.make();
      const session = yield* provider.start({
        cwd,
        env: { ...profile.env },
        ...(model !== undefined ? { model } : {}),
        ...(permissionMode !== undefined ? { permissionMode } : {}),
        ...(resume !== undefined ? { resume } : {}),
        askPermission: (toolName, input) => perms.ask(agentId, toolName, toolDetail(toolName, input)),
      }).pipe(Scope.extend(scope));

      yield* patch(agentId, (agent) => ({
        ...agent,
        session,
        scope,
        timeline: Tl.startEpoch(agent.timeline),
      }));

      yield* session.events.pipe(
        Stream.runForEach((event) => applyEvent(agentId, event)),
        Effect.forkIn(scope),
      );

      return session;
    });

  /** Records outlive the daemon, so a prompt to a restored agent picks its provider session back up. */
  const revive = (summary: AgentSummary) =>
    Effect.gen(function* () {
      const profile = config.profiles.find((candidate) => candidate.id === summary.profileId);
      if (!profile) return yield* new UnknownProfile({ profileId: summary.profileId });
      return yield* spawn(summary.id, profile, summary.cwd, summary.model, undefined, summary.providerSessionId);
    });

  const promptAgent = (agentId: string, text: string) =>
    Effect.gen(function* () {
      const agent = yield* require_(agentId);
      const session = agent.session ?? (yield* revive(agent.summary));

      const id = yield* Effect.sync(() => randomUUID());
      const ts = yield* Effect.sync(() => Date.now());

      const emitted = yield* Ref.modify(agents, (map) => {
        const current = map.get(agentId);
        if (!current) return [null, map] as const;
        const [timeline, item] = Tl.addUserText(current.timeline, text, id, ts);
        const summary: AgentSummary = { ...current.summary, maxSeq: timeline.seq, epoch: timeline.epoch, updatedAt: ts };
        return [{ item, epoch: timeline.epoch }, new Map(map).set(agentId, { ...current, timeline, summary })] as const;
      });

      if (emitted) yield* emit(agentId, emitted.item, emitted.epoch);
      /** A new turn opens a fresh panel: what already finished belongs to the turn that spawned it. */
      yield* patch(agentId, (current) => ({
        ...current,
        turn: true,
        tasks: new Map([...current.tasks].filter(([, task]) => task.status === "running")),
      }));
      yield* publishTasks(agentId);
      yield* session.prompt(text);
      yield* setStatus(agentId, "running");
    });

  /** The provider has no compaction channel, so it rides the prompt stream as the CLI's own command. */
  const compactAgent = (agentId: string) =>
    Effect.gen(function* () {
      const agent = yield* require_(agentId);
      const session = agent.session ?? (yield* revive(agent.summary));
      yield* patch(agentId, (current) => ({ ...current, compaction: "pending", turn: true }));
      yield* session.prompt("/compact");
      yield* setStatus(agentId, "compacting");
    });

  const service: AgentsService = {
    list: Effect.map(Ref.get(agents), (map) =>
      [...map.values()].map((agent) => agent.summary).sort((a, b) => b.updatedAt - a.updatedAt),
    ),

    timelineOf: (agentId) => Effect.map(require_(agentId), (agent) => agent.timeline),

    create: (input) =>
      Effect.gen(function* () {
        const profile = config.profiles.find((candidate) => candidate.id === input.profileId);
        if (!profile) return yield* new UnknownProfile({ profileId: input.profileId });

        const id = yield* Effect.sync(() => randomUUID());
        const now = yield* Effect.sync(() => Date.now());
        const summary: AgentSummary = {
          id,
          title: titleFrom(input.prompt),
          cwd: input.cwd,
          profileId: profile.id,
          status: "initializing",
          epoch: 0,
          maxSeq: 0,
          createdAt: now,
          updatedAt: now,
          ...(input.model !== undefined ? { model: input.model } : {}),
        };

        yield* Ref.update(agents, (map) =>
          new Map(map).set(id, {
            summary,
            timeline: Tl.empty,
            session: null,
            scope: null,
            compaction: "none",
            tasks: new Map(),
            turn: false,
          }),
        );

        yield* spawn(id, profile, input.cwd, input.model, input.permissionMode, undefined);
        yield* promptAgent(id, input.prompt);
        return yield* Effect.map(require_(id), (agent) => agent.summary);
      }),

    prompt: promptAgent,

    compact: compactAgent,

    tasks: Effect.map(Ref.get(agents), (map) =>
      [...map.values()]
        .filter((agent) => agent.tasks.size > 0)
        .map((agent) => ({ agentId: agent.summary.id, tasks: [...agent.tasks.values()] })),
    ),

    stopTask: (agentId, taskId) =>
      Effect.gen(function* () {
        const agent = yield* require_(agentId);
        if (agent.session) yield* agent.session.stopTask(taskId);
      }),

    interrupt: (agentId) =>
      Effect.gen(function* () {
        const agent = yield* require_(agentId);
        yield* perms.denyAllFor(agentId);
        if (agent.session) yield* agent.session.interrupt;
      }),

    close: (agentId) =>
      Effect.gen(function* () {
        const agent = yield* require_(agentId);
        yield* perms.denyAllFor(agentId);
        if (agent.scope) yield* Scope.close(agent.scope, Exit.void);
        yield* patch(agentId, (current) => ({ ...current, session: null, scope: null, tasks: new Map(), turn: false }));
        yield* publishTasks(agentId);
        yield* setStatus(agentId, "closed");
      }),
  };

  return service;
});

export const AgentsLive = Layer.effect(Agents, make);
