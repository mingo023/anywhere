import { Schema } from "effect";
import { ProviderId } from "./launch.js";

export const Ticket = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  prompt: Schema.String,
  provider: ProviderId,
  folder: Schema.String,
  base: Schema.optional(Schema.String),
  state: Schema.Literal("todo", "queued", "live", "closed"),
  order: Schema.Int,
  status: Schema.optional(Schema.Literal("starting", "working", "needsYou", "done", "failed", "stopped", "idle")),
  why: Schema.optional(Schema.String),
  agentId: Schema.optional(Schema.String),
  terminalId: Schema.optional(Schema.String),
  worktree: Schema.optional(Schema.String),
  createdAt: Schema.Number,
  changedAt: Schema.Number,
  closedAt: Schema.optional(Schema.Number),
  closedWhy: Schema.optional(Schema.Literal("closed", "merged", "deleted")),
  relaunch: Schema.optional(Schema.Boolean),
  number: Schema.optional(Schema.Int),
  priority: Schema.optional(Schema.Int),
  labels: Schema.optional(Schema.Array(Schema.Literal("Bug", "Feature", "Docs", "Infra"))),
});
export type Ticket = typeof Ticket.Type;
