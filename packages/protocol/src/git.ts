import { Schema } from "effect";

export const CAP_GIT = "git.v1";

const Tree = Schema.NonEmptyString;
const Count = Schema.Int.pipe(Schema.nonNegative());
const Paths = Schema.NonEmptyArray(Schema.NonEmptyString);

export const DiffAt = Schema.Union(
  Schema.Struct({ kind: Schema.Literal("working", "staged") }),
  Schema.Struct({ kind: Schema.Literal("commit"), rev: Schema.NonEmptyString }),
  Schema.Struct({ kind: Schema.Literal("base"), rev: Schema.optional(Schema.String) }),
);

export const FileStatus = Schema.Literal("M", "A", "D", "R", "U", "?");
export const GitFile = Schema.Struct({
  path: Schema.String,
  oldPath: Schema.optional(Schema.String),
  status: FileStatus,
  staged: Schema.Boolean,
  unstaged: Schema.Boolean,
  added: Schema.optional(Schema.Int),
  removed: Schema.optional(Schema.Int),
  binary: Schema.optional(Schema.Boolean),
});
export const CommitRef = Schema.Struct({ sha: Schema.String, subject: Schema.String });
export const DiffRow = Schema.Struct({
  k: Schema.Literal("h", "c", "a", "d"),
  o: Schema.optional(Schema.Int),
  n: Schema.optional(Schema.Int),
  t: Schema.String,
  w: Schema.optional(Schema.Array(Schema.Tuple(Schema.Int, Schema.Int))),
  cut: Schema.optional(Schema.Boolean),
});
export const LogCommit = Schema.Struct({
  sha: Schema.String,
  parents: Schema.Array(Schema.String),
  author: Schema.String,
  at: Schema.Number,
  subject: Schema.String,
});
export const CommitFile = Schema.Struct({
  path: Schema.String,
  oldPath: Schema.optional(Schema.String),
  status: FileStatus,
  added: Schema.optional(Schema.Int),
  removed: Schema.optional(Schema.Int),
});

export const GitClient = [
  Schema.Struct({
    type: Schema.Literal("git.status"),
    id: Schema.String,
    tree: Tree,
    cursor: Schema.optional(Count),
    limit: Schema.optional(Schema.Int.pipe(Schema.between(1, 2000))),
  }),
  Schema.Struct({ type: Schema.Literal("git.watch"), id: Schema.String, trees: Schema.Array(Tree) }),
  Schema.Struct({
    type: Schema.Literal("git.diff"),
    id: Schema.String,
    tree: Tree,
    path: Schema.NonEmptyString,
    at: DiffAt,
    context: Schema.optional(Count),
    ignoreWhitespace: Schema.optional(Schema.Boolean),
    maxLines: Schema.optional(Schema.Int.pipe(Schema.positive())),
  }),
  Schema.Struct({
    type: Schema.Literal("git.log"),
    id: Schema.String,
    tree: Tree,
    all: Schema.optional(Schema.Boolean),
    skip: Schema.optional(Count),
    limit: Schema.optional(Schema.Int.pipe(Schema.between(1, 200))),
  }),
  Schema.Struct({ type: Schema.Literal("git.commit"), id: Schema.String, tree: Tree, sha: Schema.NonEmptyString }),
  Schema.Struct({ type: Schema.Literal("git.stage"), id: Schema.String, tree: Tree, paths: Paths, staged: Schema.Boolean }),
  Schema.Struct({ type: Schema.Literal("git.discard"), id: Schema.String, tree: Tree, paths: Paths }),
  Schema.Struct({
    type: Schema.Literal("git.commit.create"),
    id: Schema.String,
    tree: Tree,
    message: Schema.String,
    amend: Schema.optional(Schema.Boolean),
    all: Schema.optional(Schema.Boolean),
  }),
  Schema.Struct({ type: Schema.Literal("git.push"), id: Schema.String, tree: Tree, lease: Schema.optional(Schema.Boolean) }),
  Schema.Struct({ type: Schema.Literal("git.pull"), id: Schema.String, tree: Tree }),
] as const;

export const GitServer = [
  Schema.Struct({
    type: Schema.Literal("git.status"),
    id: Schema.String,
    tree: Schema.String,
    rev: Schema.String,
    branch: Schema.optional(Schema.String),
    detached: Schema.optional(Schema.Boolean),
    upstream: Schema.optional(Schema.String),
    base: Schema.optional(Schema.String),
    ahead: Schema.Int,
    behind: Schema.Int,
    unpushed: Schema.optional(Schema.Int),
    totals: Schema.Struct({ files: Schema.Int, added: Schema.Int, removed: Schema.Int, staged: Schema.Int }),
    files: Schema.Array(GitFile),
    commits: Schema.Array(CommitRef),
    nextCursor: Schema.optional(Schema.Int),
  }),
  Schema.Struct({ type: Schema.Literal("git.changed"), tree: Schema.String, rev: Schema.String }),
  Schema.Struct({ type: Schema.Literal("gh.changed"), tree: Schema.String }),
  Schema.Struct({
    type: Schema.Literal("git.diff"),
    id: Schema.String,
    tree: Schema.String,
    path: Schema.String,
    oldPath: Schema.optional(Schema.String),
    language: Schema.String,
    added: Schema.Int,
    removed: Schema.Int,
    binary: Schema.optional(Schema.Boolean),
    tooLarge: Schema.optional(Schema.Boolean),
    image: Schema.optional(Schema.Boolean),
    lines: Schema.Array(DiffRow),
    truncated: Schema.optional(Schema.Struct({ shown: Schema.Int, total: Schema.Int })),
  }),
  Schema.Struct({
    type: Schema.Literal("git.log"),
    id: Schema.String,
    tips: Schema.Struct({
      head: Schema.optional(Schema.String),
      branch: Schema.optional(Schema.String),
      upstream: Schema.optional(Schema.String),
      base: Schema.optional(Schema.String),
    }),
    commits: Schema.Array(LogCommit),
    more: Schema.Boolean,
  }),
  Schema.Struct({
    type: Schema.Literal("git.commit"),
    id: Schema.String,
    sha: Schema.String,
    subject: Schema.String,
    body: Schema.optional(Schema.String),
    author: Schema.String,
    at: Schema.Number,
    files: Schema.Array(CommitFile),
  }),
  Schema.Struct({ type: Schema.Literal("git.committed"), id: Schema.String, sha: Schema.String }),
] as const;
