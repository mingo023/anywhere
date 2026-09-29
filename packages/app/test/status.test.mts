import { test } from "node:test";
import assert from "node:assert/strict";
import type { AgentSummary } from "@pocket/protocol";
import { byUrgency, look } from "../src/status.ts";

const agent = (id: string, status: AgentSummary["status"], more: Partial<AgentSummary> = {}) =>
  ({ id, status, attached: true, updatedAt: 0, ...more }) as AgentSummary;

const ids = (list: AgentSummary[]) => list.sort(byUrgency).map((a) => a.id);

test("rows sort needs you, failed, done, working, idle", () => {
  const list = [
    agent("idle", "idle"),
    agent("working", "working"),
    agent("done", "done"),
    agent("failed", "done", { failed: true }),
    agent("needs", "needsYou"),
  ];
  assert.deepEqual(ids(list), ["needs", "failed", "done", "working", "idle"]);
});

test("rows with the same status sort by most recent update", () => {
  const list = [agent("old", "working", { updatedAt: 1 }), agent("new", "working", { updatedAt: 2 })];
  assert.deepEqual(ids(list), ["new", "old"]);
});

test("a not-attached agent sorts and looks idle whatever its status", () => {
  const detached = agent("detached", "needsYou", { attached: false, updatedAt: 1 });
  assert.deepEqual(look(detached), look(agent("idle", "idle")));
  assert.deepEqual(ids([detached, agent("working", "working")]), ["working", "detached"]);
});

test("each status has its glossary label and theme tone", () => {
  const looks = [
    agent("a", "needsYou"),
    agent("b", "done", { failed: true }),
    agent("c", "done"),
    agent("d", "working"),
    agent("e", "idle"),
  ].map(look);
  assert.deepEqual(
    looks.map(({ tone, label }) => [tone, label]),
    [
      ["warn", "Needs you"],
      ["error", "Failed"],
      ["accent", "Done"],
      ["ok", "Working"],
      ["muted", undefined],
    ],
  );
});
