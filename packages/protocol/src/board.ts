import { Schema } from "effect";
import { ProviderId } from "./launch.js";

const Label = Schema.Literal("Bug", "Feature", "Docs", "Infra");

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
  labels: Schema.optional(Schema.Array(Label)),
});
export type Ticket = typeof Ticket.Type;


/** What `board.save` carries; no `id` creates. */
export const TicketDraft = Schema.Struct({
  id: Schema.optional(Schema.String),
  title: Schema.String,
  prompt: Schema.String,
  provider: ProviderId,
  folder: Schema.String,
  base: Schema.optional(Schema.String),
  priority: Schema.optional(Schema.Int),
  labels: Schema.optional(Schema.Array(Label)),
});
export type TicketDraft = typeof TicketDraft.Type;

const TicketRef = (type: "board.delete" | "board.start" | "board.close" | "board.reopen") =>
  Schema.Struct({ type: Schema.Literal(type), id: Schema.String, ticketId: Schema.NonEmptyString });

export const BoardClient = [
  Schema.Struct({ type: Schema.Literal("board.save"), id: Schema.String, ticket: TicketDraft, start: Schema.optional(Schema.Boolean) }),
  TicketRef("board.delete"),
  TicketRef("board.start"),
  TicketRef("board.close"),
  TicketRef("board.reopen"),
  Schema.Struct({
    type: Schema.Literal("board.move"),
    id: Schema.String,
    ticketId: Schema.NonEmptyString,
    column: Schema.Literal("todo", "queued"),
    before: Schema.optional(Schema.String),
  }),
  Schema.Struct({ type: Schema.Literal("board.limit"), id: Schema.String, limit: Schema.Int }),
] as const;
