import { randomUUID } from "node:crypto";
import type { Decision, PermissionRequest, ToolDetail } from "@pocket/protocol";
import { Context, Deferred, Duration, Effect, Layer, PubSub, Ref } from "effect";
import { Broadcast } from "./broadcast.js";

const TIMEOUT = Duration.minutes(10);

type Pending = { readonly request: PermissionRequest; readonly deferred: Deferred.Deferred<Decision> };

export interface PermissionsService {
  /** Parks the provider until a client answers; an unanswered ask must deny, never hang the agent. */
  readonly ask: (agentId: string, toolName: string, detail: ToolDetail) => Effect.Effect<Decision>;
  readonly resolve: (requestId: string, decision: Decision) => Effect.Effect<boolean>;
  readonly denyAllFor: (agentId: string) => Effect.Effect<void>;
  readonly open: Effect.Effect<ReadonlyArray<PermissionRequest>>;
}

export class Permissions extends Context.Tag("Permissions")<Permissions, PermissionsService>() {}

const make = Effect.gen(function* () {
  const hub = yield* Broadcast;
  const pending = yield* Ref.make(new Map<string, Pending>());

  const settle = (requestId: string, decision: Decision): Effect.Effect<boolean> =>
    Effect.gen(function* () {
      const entry = (yield* Ref.get(pending)).get(requestId);
      if (!entry) return false;
      yield* Ref.update(pending, (map) => {
        const next = new Map(map);
        next.delete(requestId);
        return next;
      });
      yield* Deferred.succeed(entry.deferred, decision);
      yield* PubSub.publish(hub, { type: "permission.resolved", requestId, decision });
      return true;
    });

  const service: PermissionsService = {
    ask: (agentId, toolName, detail) =>
      Effect.gen(function* () {
        const requestId = yield* Effect.sync(() => randomUUID());
        const request: PermissionRequest = { requestId, agentId, toolName, detail };
        const deferred = yield* Deferred.make<Decision>();

        yield* Ref.update(pending, (map) => new Map(map).set(requestId, { request, deferred }));
        yield* PubSub.publish(hub, { type: "permission.request", request });

        const outcome = yield* Deferred.await(deferred).pipe(
          Effect.timeoutTo({ duration: TIMEOUT, onTimeout: () => "timeout" as const, onSuccess: (d) => d }),
          Effect.onInterrupt(() => settle(requestId, "deny")),
        );

        if (outcome !== "timeout") return outcome;
        yield* settle(requestId, "deny");
        return "deny";
      }),

    resolve: settle,

    denyAllFor: (agentId) =>
      Effect.gen(function* () {
        const ids = [...(yield* Ref.get(pending)).values()]
          .filter((entry) => entry.request.agentId === agentId)
          .map((entry) => entry.request.requestId);
        yield* Effect.forEach(ids, (id) => settle(id, "deny"), { discard: true });
      }),

    open: Effect.map(Ref.get(pending), (map) => [...map.values()].map((entry) => entry.request)),
  };

  return service;
});

export const PermissionsLive = Layer.effect(Permissions, make);
