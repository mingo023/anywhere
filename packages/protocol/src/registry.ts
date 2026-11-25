import { Schema } from "effect";

export const CAP_REGISTRY_V2 = "registry.v2";

export const Repo = Schema.Struct({
  name: Schema.String,
  color: Schema.Number,
  setup: Schema.String,
  teardown: Schema.String,
  launch: Schema.optional(
    Schema.Struct({ provider: Schema.String, model: Schema.String, effort: Schema.String, access: Schema.String, account: Schema.String, options: Schema.optional(Schema.Record({ key: Schema.String, value: Schema.String })) }),
  ),
  dev_url: Schema.String,
  icon: Schema.NullOr(Schema.Union(Schema.Struct({ emoji: Schema.String }), Schema.Struct({ image: Schema.String }))),
  gh_account: Schema.String,
  accounts: Schema.Record({ key: Schema.String, value: Schema.String }),
  hidden: Schema.Array(Schema.String),
});
export type Repo = typeof Repo.Type;

export const SharedSettings = Schema.Struct({
  git: Schema.Struct({ remote: Schema.String, commit_all: Schema.NullOr(Schema.Boolean), force_with_lease: Schema.Boolean }),
  access: Schema.String,
  worktree_root: Schema.String,
});
export type SharedSettings = typeof SharedSettings.Type;

export const RegistryClient = [
  Schema.Struct({ type: Schema.Literal("project.add"), id: Schema.String, path: Schema.NonEmptyString, repo: Schema.optional(Repo) }),
  Schema.Struct({ type: Schema.Literal("project.remove"), id: Schema.String, path: Schema.NonEmptyString }),
  Schema.Struct({ type: Schema.Literal("project.set"), id: Schema.String, path: Schema.NonEmptyString, repo: Repo }),
  Schema.Struct({ type: Schema.Literal("worktree.track"), id: Schema.String, path: Schema.NonEmptyString, tracked: Schema.Boolean }),
  Schema.Struct({ type: Schema.Literal("settings.shared"), id: Schema.String, settings: SharedSettings }),
] as const;

export const RegistryServer = [
  Schema.Struct({
    type: Schema.Literal("registry.list"),
    projects: Schema.Array(Schema.Struct({ id: Schema.String, path: Schema.String, repo: Repo })),
    tracked: Schema.Array(Schema.String),
    settings: SharedSettings,
  }),
] as const;
