import { test } from "node:test";
import assert from "node:assert/strict";
import type { FileDiff, TimelineItem } from "@pocket/protocol";
import { changedFiles, changeRows, totals } from "../src/changes.ts";

const diff = (additions: number, deletions: number): FileDiff => ({ lines: [], additions, deletions });

let next = 0;
const edit = (path: string, change?: FileDiff, kind: "edit" | "write" = "edit") =>
  ({
    kind: "tool",
    id: `i${next++}`,
    call: { toolUseId: `t${next}`, name: kind, status: "ok", detail: { kind, path, diff: change } },
  }) as TimelineItem;

test("totals use the latest edit per file", () => {
  const files = changedFiles([edit("a.ts", diff(5, 1)), edit("a.ts", diff(7, 2)), edit("b.ts", diff(3, 0), "write")]);
  assert.deepEqual(totals(files), { added: 10, removed: 2 });
});

test("edits are listed oldest first", () => {
  const first = diff(1, 0);
  const second = diff(2, 0);
  const [file] = changedFiles([edit("a.ts", first), edit("a.ts", second)]);
  assert.deepEqual(file?.edits, [first, second]);
});

test("files keep first-edit order", () => {
  const files = changedFiles([edit("b.ts", diff(1, 0)), edit("a.ts", diff(1, 0)), edit("b.ts", diff(2, 0))]);
  assert.deepEqual(
    files.map((f) => f.path),
    ["b.ts", "a.ts"],
  );
});

test("an edit without a diff is not a change", () => {
  assert.deepEqual(changedFiles([edit("a.ts")]), []);
});

test("a closed file shows one row", () => {
  const files = changedFiles([edit("a.ts", diff(1, 0)), edit("a.ts", diff(2, 0))]);
  assert.deepEqual(
    changeRows(files, new Set()).map((r) => r.key),
    ["f:a.ts"],
  );
});

test("an open file shows a row per edit", () => {
  const files = changedFiles([edit("a.ts", diff(1, 0)), edit("a.ts", diff(2, 0)), edit("b.ts", diff(1, 0))]);
  const rows = changeRows(files, new Set(["a.ts"]));
  assert.deepEqual(
    rows.map((r) => r.key),
    ["f:a.ts", "e:a.ts:0", "e:a.ts:1", "f:b.ts"],
  );
  assert.deepEqual(
    rows.flatMap((r) => (r.kind === "edit" ? [`${r.index + 1} of ${r.count}`] : [])),
    ["1 of 2", "2 of 2"],
  );
});

test("fifteen hundred files build one uniquely keyed row each", () => {
  const items = Array.from({ length: 1500 }, (_, i) => edit(`src/file${i}.ts`, diff(1, 1)));
  const rows = changeRows(changedFiles(items), new Set());
  assert.equal(rows.length, 1500);
  assert.equal(new Set(rows.map((r) => r.key)).size, 1500);
});
