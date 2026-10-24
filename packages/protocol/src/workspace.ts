import { Schema } from "effect";
import { ProviderId } from "./launch.js";

export const CAP_WORKSPACE = "workspace.v1";

export const Branch = Schema.Struct({ name: Schema.String, at: Schema.Number });
export const Usage = Schema.Struct({ pct: Schema.Number, resetsAt: Schema.optional(Schema.Number) });
export const Account = Schema.Struct({
  id: Schema.String,
  provider: Schema.String,
  name: Schema.String,
  plan: Schema.optional(Schema.String),
  email: Schema.optional(Schema.String),
  signedIn: Schema.Boolean,
  isDefault: Schema.optional(Schema.Boolean),
  fiveHour: Schema.optional(Usage),
  week: Schema.optional(Usage),
});
export type Account = typeof Account.Type;

export const WorkspaceClient = [
  Schema.Struct({
    type: Schema.Literal("worktree.create"),
    id: Schema.String,
    requestId: Schema.String.pipe(Schema.minLength(1), Schema.maxLength(64)),
    project: Schema.NonEmptyString,
    name: Schema.String,
    base: Schema.optional(Schema.String),
    branch: Schema.optional(Schema.String),
    pr: Schema.optional(Schema.Int.pipe(Schema.positive())),
    setup: Schema.optional(Schema.Boolean),
  }),
  Schema.Struct({ type: Schema.Literal("worktree.delete"), id: Schema.String, path: Schema.NonEmptyString, deleteBranch: Schema.optional(Schema.Boolean), force: Schema.optional(Schema.Boolean) }),
  Schema.Struct({ type: Schema.Literal("branch.list"), id: Schema.String, tree: Schema.NonEmptyString }),
  Schema.Struct({ type: Schema.Literal("account.list"), id: Schema.String, provider: Schema.optional(ProviderId) }),
  Schema.Struct({ type: Schema.Literal("agent.account"), id: Schema.String, agentId: Schema.String, account: Schema.NonEmptyString }),
] as const;

export const WorkspaceServer = [
  Schema.Struct({
    type: Schema.Literal("worktree.progress"),
    id: Schema.String,
    requestId: Schema.String,
    step: Schema.String,
    note: Schema.optional(Schema.String),
  }),
  Schema.Struct({ type: Schema.Literal("worktree.created"), id: Schema.String, requestId: Schema.String, path: Schema.String, branch: Schema.String }),
  Schema.Struct({
    type: Schema.Literal("branch.list"),
    id: Schema.String,
    current: Schema.optional(Schema.String),
    local: Schema.Array(Branch),
    remote: Schema.Array(Branch),
  }),
  Schema.Struct({ type: Schema.Literal("account.list"), id: Schema.String, accounts: Schema.Array(Account) }),
] as const;
