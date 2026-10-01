import { Schema } from "effect";
import { AgentSummary, Decision, PermissionRequest, TimelineItem } from "./timeline.js";

export const Range = Schema.Struct({ min: Schema.Int, max: Schema.Int });
export const Scope = Schema.Literal("observe", "drive", "approve", "spawn", "owner");

export const ClientMessage = Schema.Union(
  Schema.Struct({
    type: Schema.Literal("hello"),
    id: Schema.String,
    token: Schema.optional(Schema.String),
    clientId: Schema.String,
    protocolVersion: Schema.Number,
    caps: Schema.optional(Schema.Array(Schema.String)),
    protocol: Schema.optional(Range),
  }),
  Schema.Struct({
    type: Schema.Literal("pair"),
    id: Schema.String,
    code: Schema.String,
    name: Schema.String,
    platform: Schema.Literal("ios", "android"),
    protocol: Schema.optional(Range),
  }),
  Schema.Struct({ type: Schema.Literal("agent.list"), id: Schema.String }),
  Schema.Struct({ type: Schema.Literal("pair.begin"), id: Schema.String }),
  Schema.Struct({ type: Schema.Literal("agent.prompt"), id: Schema.String, agentId: Schema.String, text: Schema.String }),
  Schema.Struct({ type: Schema.Literal("agent.interrupt"), id: Schema.String, agentId: Schema.String }),
  Schema.Struct({ type: Schema.Literal("agent.compact"), id: Schema.String, agentId: Schema.String }),
  Schema.Struct({ type: Schema.Literal("agent.close"), id: Schema.String, agentId: Schema.String }),
  Schema.Struct({ type: Schema.Literal("agent.view"), id: Schema.String, agentIds: Schema.Array(Schema.String) }),
  Schema.Struct({ type: Schema.Literal("agent.seen"), id: Schema.String, agentIds: Schema.Array(Schema.String) }),
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

export const CAP_HOST = "host.v1";

export const HostState = Schema.Struct({ tailnet: Schema.Boolean, keepingAwake: Schema.Boolean });
export type HostState = typeof HostState.Type;

export const ServerMessage = Schema.Union(
  Schema.Struct({
    type: Schema.Literal("hello.ok"),
    id: Schema.String,
    serverId: Schema.String,
    hostname: Schema.String,
    protocolVersion: Schema.Number,
    caps: Schema.optional(Schema.Array(Schema.String)),
    protocol: Schema.optional(Range),
    scopes: Schema.optional(Schema.Array(Scope)),
    host: Schema.optional(HostState),
  }),
  Schema.Struct({ type: Schema.Literal("pair.ok"), id: Schema.String, deviceId: Schema.String, token: Schema.String }),
  Schema.Struct({
    type: Schema.Literal("pair.offer"),
    id: Schema.String,
    url: Schema.String,
    code: Schema.String,
    expiresAt: Schema.Number,
  }),
  Schema.Struct({ type: Schema.Literal("pair.done"), deviceId: Schema.String, name: Schema.String }),
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
  Schema.Struct({
    type: Schema.Literal("error"),
    id: Schema.optional(Schema.String),
    message: Schema.String,
    code: Schema.optional(Schema.String),
  }),
  Schema.Struct({ type: Schema.Literal("host.changed"), host: HostState }),
);
export type ServerMessage = typeof ServerMessage.Type;

export const decodeClientMessage = Schema.decodeUnknownEither(ClientMessage);
export const encodeServerMessage = Schema.encodeSync(ServerMessage);
