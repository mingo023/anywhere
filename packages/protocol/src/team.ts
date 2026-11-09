import { Schema } from "effect";
import { ProviderId } from "./launch.js";

export const TeamState = Schema.Literal("planning", "proposed", "running", "review", "merging", "merged", "conflict", "failed");
export type TeamState = typeof TeamState.Type;

export const TeamTaskState = Schema.Literal("planned", "waiting", "running", "needsYou", "done", "failed", "stopped", "merged");
export type TeamTaskState = typeof TeamTaskState.Type;

export const TeamTask = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  agent: ProviderId,
  model: Schema.optional(Schema.String),
  files: Schema.Array(Schema.String),
  after: Schema.Array(Schema.String),
  allow: Schema.Array(Schema.String),
  instructions: Schema.String,
  state: TeamTaskState,
  worker: Schema.optional(Schema.String),
  branch: Schema.optional(Schema.String),
  worktree: Schema.optional(Schema.String),
  summary: Schema.optional(Schema.String),
  stat: Schema.optional(Schema.String),
  turn: Schema.optional(Schema.String),
});
export type TeamTask = typeof TeamTask.Type;

export const Team = Schema.Struct({
  lead: Schema.String,
  state: TeamState,
  error: Schema.optional(Schema.String),
  tasks: Schema.Array(TeamTask),
  ready: Schema.optional(Schema.Boolean),
  conflict: Schema.optional(Schema.Array(Schema.String)),
  repaired: Schema.optional(Schema.Boolean),
  checkout: Schema.optional(Schema.String),
  updatedAt: Schema.Number,
});
export type Team = typeof Team.Type;

// A task kept at team.start, as the user left its agent, model and allowlist.
export const TaskPick = Schema.Struct({
  id: Schema.String,
  agent: ProviderId,
  model: Schema.optional(Schema.String),
  allow: Schema.optional(Schema.Array(Schema.String)),
});
export type TaskPick = typeof TaskPick.Type;

export const TeamClient = [
  Schema.Struct({ type: Schema.Literal("team.list"), id: Schema.String }),
  Schema.Struct({ type: Schema.Literal("team.start"), id: Schema.String, lead: Schema.String, tasks: Schema.Array(TaskPick) }),
  Schema.Struct({ type: Schema.Literal("team.stop"), id: Schema.String, lead: Schema.String, task: Schema.String }),
  Schema.Struct({ type: Schema.Literal("team.merge"), id: Schema.String, lead: Schema.String }),
] as const;

export const TeamServer = [
  Schema.Struct({ type: Schema.Literal("team.list"), id: Schema.String, teams: Schema.Array(Team) }),
  Schema.Struct({ type: Schema.Literal("team.update"), team: Team }),
] as const;
