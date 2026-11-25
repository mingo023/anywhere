import { Schema } from "effect";

/** agent.prompt is acked once pocketd takes it; agent.delivery then says what came of it, and a resent id is the same prompt. */
export const CAP_DELIVERY = "delivery.v1";

/** What came of an acked agent.prompt: its turn started, it waits in the queue, or the agent didn't take it. */
export const AgentDelivery = Schema.Struct({
  type: Schema.Literal("agent.delivery"),
  id: Schema.String,
  agentId: Schema.String,
  state: Schema.Literal("started", "queued", "failed"),
  error: Schema.optional(Schema.String),
});
export type AgentDelivery = typeof AgentDelivery.Type;

/** What a session's last turn did, sub-agents' work aside. */
export const Recap = Schema.Struct({
  asked: Schema.optional(Schema.String),
  read: Schema.optional(Schema.Number),
  edited: Schema.optional(Schema.Number),
  said: Schema.optional(Schema.String),
});
export type Recap = typeof Recap.Type;
