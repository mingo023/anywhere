import { Schema } from "effect";

export const AgentStatus = Schema.Literal("initializing", "idle", "running", "compacting", "error", "closed");
export type AgentStatus = typeof AgentStatus.Type;

export const PermissionMode = Schema.Literal("default", "plan", "acceptEdits", "bypassPermissions");
export type PermissionMode = typeof PermissionMode.Type;

export const Decision = Schema.Literal("allow", "deny");
export type Decision = typeof Decision.Type;

export const DiffLine = Schema.Struct({
  number: Schema.optional(Schema.Number),
  kind: Schema.Literal("add", "del", "context"),
  text: Schema.String,
});
export type DiffLine = typeof DiffLine.Type;

export const FileDiff = Schema.Struct({
  lines: Schema.Array(DiffLine),
  additions: Schema.Number,
  deletions: Schema.Number,
  /** A write with no prior contents to compare against, so every line reads as added. */
  contentOnly: Schema.optional(Schema.Boolean),
});
export type FileDiff = typeof FileDiff.Type;

export const ToolDetail = Schema.Union(
  Schema.Struct({ kind: Schema.Literal("shell"), command: Schema.String, description: Schema.optional(Schema.String) }),
  Schema.Struct({ kind: Schema.Literal("read"), path: Schema.String }),
  Schema.Struct({ kind: Schema.Literal("edit"), path: Schema.String, diff: Schema.optional(FileDiff) }),
  Schema.Struct({ kind: Schema.Literal("write"), path: Schema.String, diff: Schema.optional(FileDiff) }),
  Schema.Struct({ kind: Schema.Literal("search"), query: Schema.String, path: Schema.optional(Schema.String) }),
  Schema.Struct({ kind: Schema.Literal("task"), description: Schema.String }),
  Schema.Struct({ kind: Schema.Literal("other"), name: Schema.String, input: Schema.Unknown }),
);
export type ToolDetail = typeof ToolDetail.Type;

export const AgentStep = Schema.Struct({
  id: Schema.String,
  kind: Schema.Literal("tool", "message", "reasoning"),
  text: Schema.String,
  detail: Schema.optional(ToolDetail),
  status: Schema.optional(Schema.Literal("running", "ok", "error")),
});
export type AgentStep = typeof AgentStep.Type;

/** The inside of a delegated run, carried on the Task call that spawned it. */
export const AgentRun = Schema.Struct({
  name: Schema.String,
  steps: Schema.Array(AgentStep),
});
export type AgentRun = typeof AgentRun.Type;

export const ToolCall = Schema.Struct({
  toolUseId: Schema.String,
  name: Schema.String,
  detail: ToolDetail,
  status: Schema.Literal("running", "ok", "error"),
  output: Schema.optional(Schema.String),
  durationMs: Schema.optional(Schema.Number),
  agentRun: Schema.optional(AgentRun),
});
export type ToolCall = typeof ToolCall.Type;

export const TaskItem = Schema.Struct({
  text: Schema.String,
  status: Schema.Literal("pending", "in_progress", "completed"),
});
export type TaskItem = typeof TaskItem.Type;

/** A subagent, background shell or workflow the provider is running for an agent. */
export const SessionTask = Schema.Struct({
  taskId: Schema.String,
  kind: Schema.Literal("agent", "shell", "workflow"),
  title: Schema.String,
  detail: Schema.optional(Schema.String),
  status: Schema.Literal("running", "ok", "error", "stopped"),
  startedAt: Schema.Number,
  endedAt: Schema.optional(Schema.Number),
  tools: Schema.optional(Schema.Number),
  tokens: Schema.optional(Schema.Number),
});
export type SessionTask = typeof SessionTask.Type;

export const TurnUsage = Schema.Struct({
  inputTokens: Schema.Number,
  outputTokens: Schema.Number,
  cacheReadTokens: Schema.optional(Schema.Number),
});
export type TurnUsage = typeof TurnUsage.Type;

export const TimelineBody = Schema.Union(
  Schema.Struct({ kind: Schema.Literal("user"), text: Schema.String }),
  Schema.Struct({ kind: Schema.Literal("assistant"), text: Schema.String }),
  Schema.Struct({ kind: Schema.Literal("thinking"), text: Schema.String }),
  Schema.Struct({ kind: Schema.Literal("tool"), call: ToolCall }),
  Schema.Struct({ kind: Schema.Literal("tasks"), items: Schema.Array(TaskItem) }),
  Schema.Struct({ kind: Schema.Literal("plan"), text: Schema.String }),
  Schema.Struct({ kind: Schema.Literal("compact"), trigger: Schema.Literal("manual", "auto") }),
  Schema.Struct({
    kind: Schema.Literal("result"),
    ok: Schema.Boolean,
    durationMs: Schema.Number,
    costUsd: Schema.optional(Schema.Number),
    error: Schema.optional(Schema.String),
    usage: Schema.optional(TurnUsage),
    turns: Schema.optional(Schema.Number),
  }),
);
export type TimelineBody = typeof TimelineBody.Type;

export const TimelineItem = Schema.extend(
  TimelineBody,
  Schema.Struct({ id: Schema.String, seq: Schema.Number, ts: Schema.Number }),
);
export type TimelineItem = typeof TimelineItem.Type;

export const AgentSummary = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  cwd: Schema.String,
  profileId: Schema.String,
  model: Schema.optional(Schema.String),
  status: AgentStatus,
  epoch: Schema.Number,
  maxSeq: Schema.Number,
  providerSessionId: Schema.optional(Schema.String),
  createdAt: Schema.Number,
  updatedAt: Schema.Number,
});
export type AgentSummary = typeof AgentSummary.Type;

export const ProfileSummary = Schema.Struct({
  id: Schema.String,
  label: Schema.String,
  provider: Schema.String,
  models: Schema.Array(Schema.String),
});
export type ProfileSummary = typeof ProfileSummary.Type;

export const PermissionRequest = Schema.Struct({
  requestId: Schema.String,
  agentId: Schema.String,
  toolName: Schema.String,
  detail: ToolDetail,
});
export type PermissionRequest = typeof PermissionRequest.Type;
