import { Schema } from "effect";

export const AgentStatus = Schema.Literal("needsYou", "done", "working", "idle", "closed");
export type AgentStatus = typeof AgentStatus.Type;

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

export const ToolCall = Schema.Struct({
  toolUseId: Schema.String,
  name: Schema.String,
  detail: ToolDetail,
  status: Schema.Literal("running", "ok", "error"),
  output: Schema.optional(Schema.String),
  durationMs: Schema.optional(Schema.Number),
});
export type ToolCall = typeof ToolCall.Type;

export const TaskItem = Schema.Struct({
  text: Schema.String,
  status: Schema.Literal("pending", "in_progress", "completed"),
});
export type TaskItem = typeof TaskItem.Type;

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
    error: Schema.optional(Schema.String),
    usage: Schema.optional(TurnUsage),
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
  terminalId: Schema.String,
  title: Schema.String,
  cwd: Schema.String,
  provider: Schema.String,
  model: Schema.optional(Schema.String),
  effort: Schema.optional(Schema.String),
  status: AgentStatus,
  failed: Schema.optional(Schema.Boolean),
  attached: Schema.Boolean,
  restore: Schema.optional(Schema.String),
  compacting: Schema.optional(Schema.Boolean),
  pinned: Schema.optional(Schema.Boolean),
  epoch: Schema.Number,
  maxSeq: Schema.Number,
  providerSessionId: Schema.optional(Schema.String),
  createdAt: Schema.Number,
  updatedAt: Schema.Number,
  project: Schema.optional(Schema.String),
  worktree: Schema.optional(Schema.String),
  mainWorktree: Schema.optional(Schema.Boolean),
  branch: Schema.optional(Schema.String),
  tokensUsed: Schema.optional(Schema.Number),
  contextWindow: Schema.optional(Schema.Number),
  origin: Schema.optional(Schema.String),
});
export type AgentSummary = typeof AgentSummary.Type;
export const RESTORE = ["resumed", "interrupted", "access_lowered", "failed"] as const;
export type RestoreOutcome = (typeof RESTORE)[number];

export const PermissionOption = Schema.Struct({ id: Schema.String, label: Schema.String });
export type PermissionOption = typeof PermissionOption.Type;

export const PermissionRequest = Schema.Struct({
  requestId: Schema.String,
  agentId: Schema.String,
  toolName: Schema.String,
  detail: ToolDetail,
  options: Schema.optional(Schema.Array(PermissionOption)),
  feedback: Schema.optional(Schema.Boolean),
});
export type PermissionRequest = typeof PermissionRequest.Type;
