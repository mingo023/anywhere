import { test } from "node:test";
import assert from "node:assert/strict";
import type { AgentSummary } from "@pocket/protocol";
import { look, restoreNotice } from "../src/status.ts";

const agent = (id: string, status: AgentSummary["status"], more: Partial<AgentSummary> = {}) =>
  ({ id, status, attached: true, updatedAt: 0, ...more }) as AgentSummary;

test("a not-attached agent looks idle whatever its status", () => {
  const detached = agent("detached", "needsYou", { attached: false, updatedAt: 1 });
  assert.deepEqual(look(detached), look(agent("idle", "idle")));
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
      ["ok", "Done"],
      ["accent", "Working"],
      ["muted", undefined],
    ],
  );
});

test("each restore outcome has its notice", () => {
  const notices = ["resumed", "interrupted", "access_lowered", "failed"].map((restore) =>
    restoreNotice(agent("a", "idle", { restore })),
  );
  assert.deepEqual(notices, ["Resumed", "Interrupted by restart", "Full access resumed as Ask", "Couldn't resume"]);
});

test("restoreNotice ignores unknown values", () => {
  assert.equal(restoreNotice(agent("a", "idle")), undefined);
  assert.equal(restoreNotice(agent("a", "idle", { restore: "rewound" })), undefined);
  assert.equal(restoreNotice(agent("a", "idle", { restore: "toString" })), undefined);
});
