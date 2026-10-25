import { Schema } from "effect";
import { LocalId } from "./locals.js";

export const CAP_TERMINAL = "terminal.v1";
export const FRAME_SNAPSHOT = 1;
export const FRAME_OUTPUT = 2;

const TermSize = Schema.Int.pipe(Schema.between(1, 1000));

export const Terminal = Schema.Struct({
  id: Schema.String,
  cwd: Schema.String,
  cols: Schema.Int,
  rows: Schema.Int,
  title: Schema.optional(Schema.String),
  localId: Schema.optional(Schema.String),
  agentId: Schema.optional(Schema.String),
  exited: Schema.optional(Schema.Boolean),
});
export type Terminal = typeof Terminal.Type;

export const TerminalClient = [
  Schema.Struct({ type: Schema.Literal("terminal.list"), id: Schema.String }),
  Schema.Struct({ type: Schema.Literal("terminal.attach"), id: Schema.String, terminalId: Schema.NonEmptyString }),
  Schema.Struct({ type: Schema.Literal("terminal.detach"), id: Schema.String, terminalId: Schema.NonEmptyString }),
  Schema.Struct({ type: Schema.Literal("terminal.close"), id: Schema.String, terminalId: Schema.NonEmptyString }),
  Schema.Struct({ type: Schema.Literal("terminal.input"), id: Schema.String, terminalId: Schema.NonEmptyString, data: Schema.String }),
  Schema.Struct({ type: Schema.Literal("terminal.resize"), id: Schema.String, terminalId: Schema.NonEmptyString, cols: TermSize, rows: TermSize }),
  Schema.Struct({
    type: Schema.Literal("terminal.create"),
    id: Schema.String,
    requestId: Schema.String.pipe(Schema.minLength(1), Schema.maxLength(64)),
    cwd: Schema.optional(Schema.String),
    localId: Schema.optional(LocalId),
    cols: Schema.optional(TermSize),
    rows: Schema.optional(TermSize),
  }),
] as const;

export const TerminalServer = [
  Schema.Struct({ type: Schema.Literal("terminal.list"), id: Schema.optional(Schema.String), terminals: Schema.Array(Terminal) }),
  Schema.Struct({ type: Schema.Literal("terminal.attached"), id: Schema.String, terminalId: Schema.String, cols: Schema.Int, rows: Schema.Int }),
  Schema.Struct({ type: Schema.Literal("terminal.resized"), terminalId: Schema.String, cols: Schema.Int, rows: Schema.Int }),
  Schema.Struct({ type: Schema.Literal("terminal.exited"), terminalId: Schema.String, code: Schema.Int }),
  Schema.Struct({ type: Schema.Literal("terminal.created"), id: Schema.String, requestId: Schema.String, terminalId: Schema.String }),
] as const;
