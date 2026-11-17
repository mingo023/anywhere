import { Schema } from "effect";

/** The host verbs and events, for owner conns only. */
export const CAP_HOSTS = "hosts.v1";

/** How ssh reaches a host: an alias from ~/.ssh/config, or an address. An empty `user` is this Mac's user. */
export const HostTarget = Schema.Union(
  Schema.Struct({ kind: Schema.Literal("alias"), name: Schema.String }),
  Schema.Struct({
    kind: Schema.Literal("address"),
    addr: Schema.String,
    port: Schema.Int,
    user: Schema.optional(Schema.String),
    keyFile: Schema.optional(Schema.String),
  }),
);
export type HostTarget = typeof HostTarget.Type;

export const HostFailure = Schema.Literal(
  "auth",
  "unknown_key",
  "key_changed",
  "no_such_host",
  "refused",
  "timed_out",
  "dropped",
  "throttled",
  "not_running",
  "unpaired",
  "path_too_long",
  "other",
);
export type HostFailure = typeof HostFailure.Type;

/** `retrying` tries again by itself; `failed` waits for `host.retry`. */
export const HostLinkState = Schema.Union(
  Schema.Struct({ state: Schema.Literal("connecting") }),
  Schema.Struct({ state: Schema.Literal("online"), version: Schema.String, uptime: Schema.Number }),
  Schema.Struct({ state: Schema.Literal("retrying", "failed"), failure: HostFailure }),
);
export type HostLinkState = typeof HostLinkState.Type;

export const Host = Schema.Struct({
  id: Schema.String,
  name: Schema.String,
  target: HostTarget,
  windows: Schema.optional(Schema.Boolean),
  /** The local end of the tunnel to the host's pocketd. */
  socket: Schema.String,
  link: HostLinkState,
});
export type Host = typeof Host.Type;

export const InstallStep = Schema.Literal("detect", "download", "verify", "unpack", "link", "service", "start");

/** `running`, `done` and `error` end an install. */
export const InstallProgress = Schema.Struct({
  kind: Schema.Literal("step", "warn", "running", "done", "error"),
  step: Schema.optional(InstallStep),
  text: Schema.optional(Schema.String),
});
export type InstallProgress = typeof InstallProgress.Type;

/** A timeline item: a card asking the user to add a host, with `hint` to fill its form. */
export const HostLinkBody = Schema.Struct({
  kind: Schema.Literal("host_link"),
  environmentId: Schema.optional(Schema.String),
  hint: Schema.optional(Schema.String),
  reason: Schema.optional(Schema.String),
});

export const HostsClient = [
  Schema.Struct({ type: Schema.Literal("host.list"), id: Schema.String }),
  Schema.Struct({ type: Schema.Literal("host.add"), id: Schema.String, target: HostTarget, name: Schema.optional(Schema.String) }),
  Schema.Struct({ type: Schema.Literal("host.remove"), id: Schema.String, hostId: Schema.NonEmptyString, uninstall: Schema.optional(Schema.Boolean) }),
  Schema.Struct({ type: Schema.Literal("host.retry"), id: Schema.String, hostId: Schema.NonEmptyString }),
  Schema.Struct({
    type: Schema.Literal("host.install"),
    id: Schema.String,
    hostId: Schema.NonEmptyString,
    update: Schema.optional(Schema.Boolean),
    archiveDir: Schema.optional(Schema.String),
  }),
  Schema.Struct({ type: Schema.Literal("host.sign_in"), id: Schema.String, hostId: Schema.NonEmptyString, password: Schema.NonEmptyString }),
] as const;

export const HostsServer = [
  /** Without an id, the whole list pushed to an owner. */
  Schema.Struct({ type: Schema.Literal("host.list"), id: Schema.optional(Schema.String), hosts: Schema.Array(Host) }),
  /** Without an id, one host pushed after it changed. */
  Schema.Struct({ type: Schema.Literal("host.update"), id: Schema.optional(Schema.String), host: Host }),
  Schema.Struct({ type: Schema.Literal("host.progress"), hostId: Schema.String, update: Schema.Boolean, progress: InstallProgress }),
] as const;
