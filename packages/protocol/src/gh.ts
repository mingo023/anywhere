import { Schema } from "effect";

export const CAP_GITHUB = "gh.v1";

const Tree = Schema.NonEmptyString;
const Number_ = Schema.Int.pipe(Schema.positive());

export const Gate = Schema.Literal(
  "done",
  "draft",
  "conflicts",
  "checksFailed",
  "checksRunning",
  "changesRequested",
  "reviewRequired",
  "checking",
  "blocked",
  "ready",
);
export const PrState = Schema.Literal("OPEN", "CLOSED", "MERGED");
export const Checks = Schema.Struct({ passed: Schema.Int, failed: Schema.Int, pending: Schema.Int });
export const Thread = Schema.Struct({
  id: Schema.String,
  path: Schema.String,
  line: Schema.optional(Schema.Int),
  left: Schema.optional(Schema.Boolean),
  outdated: Schema.Boolean,
  resolved: Schema.Boolean,
  comments: Schema.Array(
    Schema.Struct({ author: Schema.String, body: Schema.String, at: Schema.Number, hunk: Schema.optional(Schema.String) }),
  ),
});
export const PR = Schema.Struct({
  number: Schema.Int,
  title: Schema.String,
  url: Schema.String,
  state: PrState,
  draft: Schema.Boolean,
  base: Schema.String,
  head: Schema.String,
  checks: Checks,
  runs: Schema.Array(
    Schema.Struct({
      name: Schema.String,
      outcome: Schema.Literal("passed", "failed", "pending", "skipped"),
      url: Schema.optional(Schema.String),
    }),
  ),
  review: Schema.optional(Schema.Literal("APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED")),
  reviews: Schema.Array(Schema.Struct({ login: Schema.String, state: Schema.String })),
  requested: Schema.Array(Schema.String),
  mergeable: Schema.optional(Schema.Literal("MERGEABLE", "CONFLICTING", "UNKNOWN")),
  gate: Gate,
  threads: Schema.Array(Thread),
});
export type PR = typeof PR.Type;

export const GhClient = [
  Schema.Struct({ type: Schema.Literal("gh.status"), id: Schema.String, tree: Schema.optional(Schema.String) }),
  Schema.Struct({ type: Schema.Literal("gh.pr"), id: Schema.String, tree: Tree }),
  Schema.Struct({ type: Schema.Literal("gh.prs"), id: Schema.String, project: Schema.NonEmptyString }),
  Schema.Struct({
    type: Schema.Literal("gh.pr.create"),
    id: Schema.String,
    tree: Tree,
    title: Schema.NonEmptyString,
    body: Schema.String,
    draft: Schema.optional(Schema.Boolean),
    base: Schema.optional(Schema.String),
  }),
  Schema.Struct({
    type: Schema.Literal("gh.pr.merge"),
    id: Schema.String,
    tree: Tree,
    number: Number_,
    method: Schema.Literal("squash", "merge", "rebase"),
  }),
  Schema.Struct({ type: Schema.Literal("gh.pr.ready"), id: Schema.String, tree: Tree, number: Number_ }),
  Schema.Struct({ type: Schema.Literal("gh.thread.reply"), id: Schema.String, tree: Tree, thread: Schema.NonEmptyString, body: Schema.NonEmptyString }),
  Schema.Struct({ type: Schema.Literal("gh.thread.resolve"), id: Schema.String, tree: Tree, thread: Schema.NonEmptyString, resolved: Schema.Boolean }),
] as const;

export const GhServer = [
  Schema.Struct({
    type: Schema.Literal("gh.status"),
    id: Schema.String,
    state: Schema.Literal("missing", "signedOut", "ready"),
    login: Schema.optional(Schema.String),
  }),
  Schema.Struct({
    type: Schema.Literal("gh.pr"),
    id: Schema.String,
    tree: Schema.String,
    problem: Schema.optional(Schema.Literal("missing", "signedOut")),
    pr: Schema.optional(PR),
  }),
  Schema.Struct({
    type: Schema.Literal("gh.prs"),
    id: Schema.String,
    items: Schema.Array(
      Schema.Struct({
        tree: Schema.String,
        number: Schema.Int,
        title: Schema.String,
        state: PrState,
        draft: Schema.Boolean,
        gate: Gate,
        checks: Checks,
      }),
    ),
  }),
  Schema.Struct({ type: Schema.Literal("gh.pr.created"), id: Schema.String, number: Schema.Int, url: Schema.String }),
] as const;
