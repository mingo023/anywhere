import { Schema } from "effect";

export const CAP_ORCHESTRATOR = "orchestrator.v1";

/** A child session's delegation, on its AgentSummary. */
export const DelegationRef = Schema.Struct({
  taskId: Schema.String,
  status: Schema.String,
  workState: Schema.optional(Schema.String),
});
export type DelegationRef = typeof DelegationRef.Type;

export const DelegationStatus = Schema.Literal("queued", "starting", "working", "completed", "failed", "cancelled");

/** One session a chat delegated to, as its panel shows it. */
export const Delegation = Schema.Struct({
  taskId: Schema.String,
  parentId: Schema.String,
  childId: Schema.optional(Schema.String),
  title: Schema.String,
  agent: Schema.String,
  model: Schema.optional(Schema.String),
  status: DelegationStatus,
  summary: Schema.optional(Schema.String),
  error: Schema.optional(Schema.String),
  question: Schema.optional(Schema.String),
  createdAt: Schema.Number,
  settledAt: Schema.optional(Schema.Number),
  /** The host a task runs on; its `childId` then carries that host's tag, `<hostId>/<theirId>`. */
  environmentId: Schema.optional(Schema.String),
});
export type Delegation = typeof Delegation.Type;

/** A timeline item: what pocketd told the parent about its children. */
export const DeliveryBody = Schema.Struct({
  kind: Schema.Literal("delivery"),
  taskIds: Schema.Array(Schema.String),
  childIds: Schema.Array(Schema.String),
  titles: Schema.Array(Schema.String),
  agents: Schema.Array(Schema.String),
  outcome: Schema.Literal("done", "failed", "asks"),
  question: Schema.optional(Schema.String),
});

/** A message waiting for its session's turn to end. */
export const QueueEntry = Schema.Struct({
  id: Schema.String,
  text: Schema.String,
  origin: Schema.Literal("user", "wake", "question", "context"),
  state: Schema.Literal("pending", "sending", "held"),
  order: Schema.Number,
});
export type QueueEntry = typeof QueueEntry.Type;
