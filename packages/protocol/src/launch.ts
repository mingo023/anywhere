import { Schema } from "effect";

export const Access = Schema.Literal("ask", "edits", "auto", "full");
export type Access = typeof Access.Type;

export const PhoneAccess = Schema.Literal("ask", "edits", "auto");
export type PhoneAccess = typeof PhoneAccess.Type;

export const NewWorktree = Schema.Struct({
  name: Schema.optional(Schema.String),
  base: Schema.optional(Schema.String),
  branch: Schema.optional(Schema.String),
  pr: Schema.optional(Schema.String),
  copy: Schema.optional(Schema.Boolean),
  setup: Schema.optional(Schema.Boolean),
});

export const Checkout = Schema.Union(Schema.Struct({ worktree: Schema.String }), Schema.Struct({ new: NewWorktree }));

export const LaunchSpec = Schema.Struct({
  project: Schema.String,
  checkout: Checkout,
  provider: Schema.Literal("claude", "codex"),
  model: Schema.optional(Schema.String),
  effort: Schema.optional(Schema.String),
  access: Access,
  plan: Schema.Boolean,
  prompt: Schema.optional(Schema.String),
});
export type LaunchSpec = typeof LaunchSpec.Type;

export const ProviderInfo = Schema.Struct({
  id: Schema.String,
  available: Schema.Boolean,
  efforts: Schema.Array(Schema.String),
  plan: Schema.Boolean,
});
export type ProviderInfo = typeof ProviderInfo.Type;

export const ErrorCode = Schema.Literal(
  "unknown_project",
  "unknown_worktree",
  "invalid_name",
  "worktree_exists",
  "invalid_spec",
  "provider_unavailable",
  "access_not_allowed",
  "folder_not_trusted",
  "spawn_failed",
  "duplicate",
  "invalid_config",
);
export type ErrorCode = typeof ErrorCode.Type;
