import { execFileSync } from "node:child_process";
import { Effect, Layer } from "effect";
import { AgentsLive } from "./agent-manager.js";
import { BroadcastLive } from "./broadcast.js";
import { AppConfig, AppConfigLive, CONFIG_PATH } from "./config.js";
import { PermissionsLive } from "./permission-broker.js";
import { startServer } from "./ws-server.js";

const tailscaleIp = Effect.try(
  () => execFileSync("tailscale", ["ip", "-4"], { encoding: "utf8" }).trim().split("\n")[0] ?? null,
).pipe(Effect.orElseSucceed(() => null));

const InfraLive = Layer.merge(AppConfigLive, BroadcastLive);
const PermsLive = Layer.provideMerge(PermissionsLive, InfraLive);
const CoreLive = Layer.provideMerge(AgentsLive, PermsLive);

const program = Effect.gen(function* () {
  const config = yield* AppConfig;
  yield* startServer;

  const host = (yield* tailscaleIp) ?? "localhost";
  yield* Effect.sync(() => {
    console.log(`coding-pocket daemon on ws://${host}:${config.port}`);
    console.log(`token: ${config.token}`);
    console.log(`config: ${CONFIG_PATH}`);
  });

  yield* Effect.never;
});

void Effect.runPromise(program.pipe(Effect.provide(CoreLive), Effect.scoped));
