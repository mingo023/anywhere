import { Schema } from "effect";
import { Access } from "./launch.js";

export const Schedule = Schema.Union(
  Schema.Struct({ kind: Schema.Literal("days"), days: Schema.Array(Schema.Int), time: Schema.String }),
  Schema.Struct({ kind: Schema.Literal("interval"), everyMin: Schema.Int }),
);
export type Schedule = typeof Schedule.Type;

export const AutomationDraft = Schema.Struct({
  id: Schema.optional(Schema.String),
  name: Schema.String,
  prompt: Schema.String,
  provider: Schema.Literal("claude", "codex"),
  folder: Schema.String,
  schedule: Schedule,
  enabled: Schema.Boolean,
  access: Schema.optional(Access),
  newWorktree: Schema.optional(Schema.Boolean),
});

export const Automation = Schema.Struct({
  id: Schema.String,
  name: Schema.String,
  prompt: Schema.String,
  provider: Schema.Literal("claude", "codex"),
  folder: Schema.String,
  schedule: Schedule,
  enabled: Schema.Boolean,
  access: Schema.optional(Access),
  newWorktree: Schema.optional(Schema.Boolean),
  nextRunAt: Schema.optional(Schema.Number),
});
export type Automation = typeof Automation.Type;

export const RunStatus = Schema.Literal("pending", "running", "waiting", "succeeded", "failed", "skipped", "cancelled");

export const Run = Schema.Struct({
  id: Schema.String,
  automationId: Schema.String,
  status: RunStatus,
  trigger: Schema.Literal("schedule", "manual"),
  why: Schema.optional(Schema.String),
  summary: Schema.optional(Schema.String),
  startedAt: Schema.Number,
  finishedAt: Schema.optional(Schema.Number),
  agentId: Schema.optional(Schema.String),
  terminalId: Schema.optional(Schema.String),
  access: Schema.optional(Schema.String),
  worktree: Schema.optional(Schema.String),
});
export type Run = typeof Run.Type;
