import { Schema } from "effect";
import { AgentSummary, Decision, PermissionRequest, TimelineItem } from "./timeline.js";

export const ClientMessage = Schema.Union(
  Schema.Struct({
    type: Schema.Literal("hello"),
    id: Schema.String,
    token: Schema.String,
    clientId: Schema.String,
    protocolVersion: Schema.Number,
  }),
  Schema.Struct({ type: Schema.Literal("agent.list"), id: Schema.String }),
  Schema.Struct({ type: Schema.Literal("agent.prompt"), id: Schema.String, agentId: Schema.String, text: Schema.String }),
  Schema.Struct({ type: Schema.Literal("agent.interrupt"), id: Schema.String, agentId: Schema.String }),
  Schema.Struct({ type: Schema.Literal("agent.compact"), id: Schema.String, agentId: Schema.String }),
  Schema.Struct({ type: Schema.Literal("agent.close"), id: Schema.String, agentId: Schema.String }),
  Schema.Struct({
    type: Schema.Literal("agent.timeline"),
    id: Schema.String,
    agentId: Schema.String,
    sinceSeq: Schema.optional(Schema.Number),
    limit: Schema.optional(Schema.Number.pipe(Schema.lessThanOrEqualTo(500))),
  }),
  Schema.Struct({
    type: Schema.Literal("permission.resolve"),
    id: Schema.String,
    requestId: Schema.String,
    decision: Decision,
    option: Schema.optional(Schema.String),
    message: Schema.optional(Schema.String),
  }),
);
export type ClientMessage = typeof ClientMessage.Type;

export const ServerMessage = Schema.Union(
  Schema.Struct({
    type: Schema.Literal("hello.ok"),
    id: Schema.String,
    serverId: Schema.String,
    hostname: Schema.String,
    protocolVersion: Schema.Number,
  }),
  Schema.Struct({ type: Schema.Literal("agent.list"), id: Schema.optional(Schema.String), agents: Schema.Array(AgentSummary) }),
  Schema.Struct({ type: Schema.Literal("agent.update"), agent: AgentSummary }),
  Schema.Struct({ type: Schema.Literal("agent.stream"), agentId: Schema.String, epoch: Schema.Number, item: TimelineItem }),
  Schema.Struct({
    type: Schema.Literal("agent.timeline"),
    id: Schema.String,
    agentId: Schema.String,
    epoch: Schema.Number,
    items: Schema.Array(TimelineItem),
    hasOlder: Schema.Boolean,
    maxSeq: Schema.Number,
  }),
  Schema.Struct({ type: Schema.Literal("permission.request"), request: PermissionRequest }),
  Schema.Struct({ type: Schema.Literal("permission.resolved"), requestId: Schema.String, decision: Decision }),
  Schema.Struct({ type: Schema.Literal("ack"), id: Schema.String }),
  Schema.Struct({ type: Schema.Literal("error"), id: Schema.optional(Schema.String), message: Schema.String }),
);
export type ServerMessage = typeof ServerMessage.Type;

export const decodeClientMessage = Schema.decodeUnknownEither(ClientMessage);
export const encodeServerMessage = Schema.encodeSync(ServerMessage);
