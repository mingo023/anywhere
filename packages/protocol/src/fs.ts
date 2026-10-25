import { Schema } from "effect";
import { LocalId } from "./locals.js";

export const CAP_FILES = "fs.v1";

const Tree = Schema.NonEmptyString;

export const FsEntry = Schema.Struct({
  name: Schema.String,
  dir: Schema.Boolean,
  link: Schema.optional(Schema.Boolean),
  size: Schema.Number,
  mtime: Schema.Number,
  ignored: Schema.optional(Schema.Boolean),
  git: Schema.optional(Schema.String),
});

export const FsClient = [
  Schema.Struct({ type: Schema.Literal("fs.list"), id: Schema.String, tree: Tree, dir: Schema.String }),
  Schema.Struct({
    type: Schema.Literal("fs.read"),
    id: Schema.String,
    tree: Tree,
    path: Schema.NonEmptyString,
    max: Schema.optional(Schema.Int.pipe(Schema.positive())),
  }),
  Schema.Struct({
    type: Schema.Literal("fs.search"),
    id: Schema.String,
    tree: Tree,
    query: Schema.String,
    limit: Schema.optional(Schema.Int.pipe(Schema.between(1, 500))),
  }),
  Schema.Struct({
    type: Schema.Literal("attach.put"),
    id: Schema.String,
    uploadId: LocalId,
    name: Schema.NonEmptyString,
    index: Schema.Int.pipe(Schema.nonNegative()),
    data: Schema.String,
    last: Schema.Boolean,
  }),
] as const;

export const FsServer = [
  Schema.Struct({ type: Schema.Literal("fs.list"), id: Schema.String, tree: Schema.String, dir: Schema.String, entries: Schema.Array(FsEntry) }),
  Schema.Struct({
    type: Schema.Literal("fs.read"),
    id: Schema.String,
    tree: Schema.String,
    path: Schema.String,
    kind: Schema.Literal("text", "image", "binary", "tooLarge", "unreadable"),
    size: Schema.Number,
    mtime: Schema.Number,
    text: Schema.optional(Schema.String),
    truncated: Schema.optional(Schema.Boolean),
    language: Schema.optional(Schema.String),
    mime: Schema.optional(Schema.String),
    data: Schema.optional(Schema.String),
  }),
  Schema.Struct({ type: Schema.Literal("fs.search"), id: Schema.String, paths: Schema.Array(Schema.String) }),
  Schema.Struct({ type: Schema.Literal("attach.done"), id: Schema.String, uploadId: Schema.String, path: Schema.String }),
] as const;
