import { randomBytes } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { Context, Effect, Layer, Schema } from "effect";
import { ConfigInvalid } from "./errors.js";

export const Profile = Schema.Struct({
  id: Schema.String,
  label: Schema.String,
  provider: Schema.String,
  env: Schema.Record({ key: Schema.String, value: Schema.String }),
  models: Schema.Array(Schema.String),
});
export type Profile = typeof Profile.Type;

export const Config = Schema.Struct({
  token: Schema.String,
  port: Schema.Number,
  profiles: Schema.Array(Profile),
});
export type Config = typeof Config.Type;

export const POCKET_HOME = process.env.POCKET_HOME ?? join(homedir(), ".coding-pocket");
export const CONFIG_PATH = join(POCKET_HOME, "config.json");

function defaults(): Config {
  return {
    token: randomBytes(24).toString("base64url"),
    port: 4517,
    profiles: [{ id: "claude-default", label: "Claude", provider: "claude", env: {}, models: ["opus", "sonnet"] }],
  };
}

const decode = Schema.decodeUnknownEither(Config);

export class AppConfig extends Context.Tag("AppConfig")<AppConfig, Config>() {}

export const load: Effect.Effect<Config, ConfigInvalid> = Effect.gen(function* () {
  yield* Effect.sync(() => mkdirSync(POCKET_HOME, { recursive: true }));

  const raw = yield* Effect.try(() => JSON.parse(readFileSync(CONFIG_PATH, "utf8")) as unknown).pipe(
    Effect.option,
  );

  if (raw._tag === "None") {
    const fresh = defaults();
    yield* Effect.sync(() =>
      writeFileSync(CONFIG_PATH, `${JSON.stringify(fresh, null, 2)}\n`, { mode: 0o600 }),
    );
    return fresh;
  }

  const parsed = decode(raw.value);
  if (parsed._tag === "Left") return yield* new ConfigInvalid({ reason: parsed.left.message });
  return parsed.right;
});

export const AppConfigLive = Layer.effect(AppConfig, load);
