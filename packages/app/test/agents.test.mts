import { test } from "node:test";
import assert from "node:assert/strict";
import type { AgentSummary } from "@pocket/protocol";
import { applyAgentUpdate } from "../src/agents.ts";

const agent = (id: string, status: AgentSummary["status"]) => ({ id, status }) as AgentSummary;

test("a closed agent leaves the list", () => {
  const list = [agent("a", "idle"), agent("b", "running")];
  assert.deepEqual(applyAgentUpdate(list, agent("a", "closed")), [agent("b", "running")]);
});

test("an update replaces in place and a new agent goes first", () => {
  const list = [agent("a", "idle"), agent("b", "idle")];
  assert.deepEqual(applyAgentUpdate(list, agent("b", "running")), [agent("a", "idle"), agent("b", "running")]);
  assert.deepEqual(applyAgentUpdate(list, agent("c", "idle")), [agent("c", "idle"), ...list]);
});
