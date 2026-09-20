import type { ServerMessage } from "@pocket/protocol";
import { Context, Effect, Layer, PubSub } from "effect";

export class Broadcast extends Context.Tag("Broadcast")<Broadcast, PubSub.PubSub<ServerMessage>>() {}

export const BroadcastLive = Layer.effect(Broadcast, PubSub.unbounded<ServerMessage>());

export const publish = (message: ServerMessage): Effect.Effect<void, never, Broadcast> =>
  Effect.flatMap(Broadcast, (hub) => PubSub.publish(hub, message));
