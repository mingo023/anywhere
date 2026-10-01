import { test } from "node:test";
import assert from "node:assert/strict";
import type { ToolCall, ToolDetail } from "@pocket/protocol";
import { groupOpen, toolSummary } from "../src/tools.ts";

let next = 0;
const call = (detail: ToolDetail, status: ToolCall["status"] = "ok"): ToolCall => ({
  toolUseId: `t${next++}`,
  name: detail.kind,
  detail,
  status,
});
const shell = (status?: ToolCall["status"]) => call({ kind: "shell", command: "pnpm test" }, status);
const edit = (path: string) => call({ kind: "edit", path });

test("a group summary counts commands edits and failures", () => {
  const calls = [shell(), shell("error"), edit("a.ts"), call({ kind: "write", path: "b.ts" })];
  assert.equal(toolSummary(calls), "Ran 2 commands · edited 2 files · 1 failed");
});

test("edits to one file count once", () => {
  const calls = [edit("a.ts"), edit("a.ts"), call({ kind: "write", path: "a.ts" })];
  assert.equal(toolSummary(calls), "Edited 1 file");
});

test("a single call reads singular", () => {
  assert.equal(toolSummary([shell()]), "Ran 1 command");
  assert.equal(toolSummary([call({ kind: "read", path: "a.ts" })]), "Read 1 file");
  assert.equal(toolSummary([call({ kind: "search", query: "todo" })]), "Searched 1 time");
  assert.equal(toolSummary([call({ kind: "task", description: "explore" })]), "Started 1 task");
  assert.equal(toolSummary([call({ kind: "other", name: "WebFetch", input: {} })]), "Called 1 tool");
});

test("the first word is capitalized", () => {
  const calls = [call({ kind: "search", query: "a" }), call({ kind: "search", query: "b" }), shell()];
  assert.equal(toolSummary(calls), "Ran 1 command · searched 2 times");
});

test("a live group is open", () => {
  assert.equal(groupOpen([shell()], true), true);
  assert.equal(groupOpen([shell("running")], false), true);
});

test("a settled group folds", () => {
  assert.equal(groupOpen([shell(), shell("error")], false), false);
});

test("a user toggle wins over the fold", () => {
  assert.equal(groupOpen([shell("running")], true, false), false);
  assert.equal(groupOpen([shell()], false, true), true);
});
