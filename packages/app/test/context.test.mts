import { test } from "node:test";
import assert from "node:assert/strict";
import type { AgentSummary } from "@pocket/protocol";
import { contextChip } from "../src/context.ts";

const agent = (tokensUsed: number, contextWindow?: number) =>
  ({ id: "a", status: "working", attached: true, updatedAt: 0, tokensUsed, contextWindow }) as AgentSummary;

test("the chip turns warning at 75 and danger at 90", () => {
  const tones = [148_000, 150_000, 178_000, 180_000].map((used) => contextChip(agent(used, 200_000))?.tone);
  assert.deepEqual(tones, ["muted", "warning", "warning", "danger"]);
});

test("the percent rounds down, so 74.99 stays muted", () => {
  assert.deepEqual(contextChip(agent(149_980, 200_000)), { percent: 74, tone: "muted" });
});

test("the percent stops at 100 when usage passes the window", () => {
  assert.deepEqual(contextChip(agent(250_000, 200_000)), { percent: 100, tone: "danger" });
});

test("no chip until pocketd reports a window", () => {
  assert.equal(contextChip(agent(184_000)), undefined);
  assert.equal(contextChip(agent(184_000, 0)), undefined);
});
