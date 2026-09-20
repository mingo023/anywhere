import { hostname } from "node:os";
import { WebSocketServer, type WebSocket } from "ws";
import {
  PROTOCOL_VERSION,
  decodeClientMessage,
  encodeServerMessage,
  type ClientMessage,
  type ServerMessage,
} from "@pocket/protocol";
import { Effect, Mailbox, PubSub, Queue, Ref, Runtime, Stream } from "effect";
import { Agents } from "./agent-manager.js";
import { Broadcast } from "./broadcast.js";
import { AppConfig, type Config } from "./config.js";
import { describe, type AppError } from "./errors.js";
import { Permissions } from "./permission-broker.js";
import * as Tl from "./timeline.js";

const DEFAULT_PAGE = 200;

type Send = (message: ServerMessage) => Effect.Effect<void>;

const handle = (
  message: Exclude<ClientMessage, { type: "hello" }>,
  config: Config,
  send: Send,
): Effect.Effect<void, AppError, Agents | Permissions> =>
  Effect.gen(function* () {
    const agents = yield* Agents;
    const perms = yield* Permissions;

    switch (message.type) {
      case "profile.list":
        return yield* send({
          type: "profile.list",
          id: message.id,
          profiles: config.profiles.map(({ id, label, provider, models }) => ({ id, label, provider, models })),
        });

      case "agent.list":
        return yield* send({ type: "agent.list", id: message.id, agents: yield* agents.list });

      case "agent.create": {
        const agent = yield* agents.create({
          cwd: message.cwd,
          profileId: message.profileId,
          prompt: message.prompt,
          ...(message.model !== undefined ? { model: message.model } : {}),
          ...(message.permissionMode !== undefined ? { permissionMode: message.permissionMode } : {}),
        });
        return yield* send({ type: "agent.update", agent });
      }

      case "agent.prompt":
        yield* agents.prompt(message.agentId, message.text);
        return yield* send({ type: "ack", id: message.id });

      case "agent.compact":
        yield* agents.compact(message.agentId);
        return yield* send({ type: "ack", id: message.id });

      case "agent.interrupt":
        yield* agents.interrupt(message.agentId);
        return yield* send({ type: "ack", id: message.id });

      case "agent.close":
        yield* agents.close(message.agentId);
        return yield* send({ type: "ack", id: message.id });

      case "task.stop":
        yield* agents.stopTask(message.agentId, message.taskId);
        return yield* send({ type: "ack", id: message.id });

      case "agent.timeline": {
        const timeline = yield* agents.timelineOf(message.agentId);
        const { items, hasOlder } = Tl.page(timeline, message.sinceSeq ?? 0, message.limit ?? DEFAULT_PAGE);
        return yield* send({
          type: "agent.timeline",
          id: message.id,
          agentId: message.agentId,
          epoch: timeline.epoch,
          items,
          hasOlder,
          maxSeq: timeline.seq,
        });
      }

      case "permission.resolve": {
        const resolved = yield* perms.resolve(message.requestId, message.decision);
        return yield* send(
          resolved
            ? { type: "ack", id: message.id }
            : { type: "error", id: message.id, message: "Permission request is no longer open" },
        );
      }
    }
  });

const connection = (socket: WebSocket, config: Config) =>
  Effect.gen(function* () {
    const hub = yield* Broadcast;
    const perms = yield* Permissions;
    const agents = yield* Agents;
    const runtime = yield* Effect.runtime<never>();

    const inbox = yield* Mailbox.make<string>();
    const authed = yield* Ref.make(false);

    const send: Send = (message) =>
      Effect.sync(() => {
        if (socket.readyState === socket.OPEN) socket.send(JSON.stringify(encodeServerMessage(message)));
      }).pipe(Effect.ignore);

    socket.on("message", (raw) => void Runtime.runPromise(runtime)(inbox.offer(String(raw)).pipe(Effect.ignore)));
    socket.on("close", () => void Runtime.runPromise(runtime)(inbox.end));
    socket.on("error", () => void Runtime.runPromise(runtime)(inbox.end));

    const subscription = yield* PubSub.subscribe(hub);
    yield* Queue.take(subscription).pipe(
      Effect.flatMap((message) =>
        Effect.flatMap(Ref.get(authed), (ok) => (ok ? send(message) : Effect.void)),
      ),
      Effect.forever,
      Effect.forkScoped,
    );

    const onRaw = (raw: string) =>
      Effect.gen(function* () {
        const json = yield* Effect.try(() => JSON.parse(raw) as unknown).pipe(Effect.option);
        if (json._tag === "None") return yield* send({ type: "error", message: "Malformed message" });

        const parsed = decodeClientMessage(json.value);
        if (parsed._tag === "Left") return yield* send({ type: "error", message: "Malformed message" });
        const message = parsed.right;

        if (message.type === "hello") {
          if (message.token !== config.token || message.protocolVersion !== PROTOCOL_VERSION) {
            yield* send({ type: "error", id: message.id, message: "Rejected" });
            return yield* Effect.sync(() => socket.close());
          }
          yield* Ref.set(authed, true);
          yield* send({
            type: "hello.ok",
            id: message.id,
            serverId: hostname(),
            hostname: hostname(),
            protocolVersion: PROTOCOL_VERSION,
          });
          yield* send({ type: "agent.list", agents: yield* agents.list });
          for (const entry of yield* agents.tasks) yield* send({ type: "agent.tasks", ...entry });
          for (const request of yield* perms.open) yield* send({ type: "permission.request", request });
          return;
        }

        if (!(yield* Ref.get(authed))) {
          return yield* send({ type: "error", id: message.id, message: "Not authenticated" });
        }

        yield* handle(message, config, send).pipe(
          Effect.catchAll((error) => send({ type: "error", id: message.id, message: describe(error) })),
        );
      });

    yield* Mailbox.toStream(inbox).pipe(Stream.runForEach(onRaw));
  }).pipe(Effect.scoped);

export const startServer = Effect.gen(function* () {
  const config = yield* AppConfig;
  const runtime = yield* Effect.runtime<Agents | Permissions | Broadcast>();

  const wss = yield* Effect.acquireRelease(
    Effect.sync(() => new WebSocketServer({ port: config.port, host: "0.0.0.0" })),
    (server) => Effect.async<void>((resume) => void server.close(() => resume(Effect.void))),
  );

  yield* Effect.sync(() =>
    wss.on("connection", (socket) => void Runtime.runPromise(runtime)(connection(socket, config))),
  );
});
