import { test } from "node:test";
import assert from "node:assert/strict";
import type { AgentSummary } from "@pocket/protocol";
import { allSessions, backLabel, needsYouElsewhere, summary, upNext } from "../src/order.ts";

const agent = (id: string, status: AgentSummary["status"], more: Partial<AgentSummary> = {}) =>
  ({ id, status, attached: true, createdAt: 0, updatedAt: 0, ...more }) as AgentSummary;

const ids = (list: readonly AgentSummary[]) => list.map((a) => a.id);

test("all sessions stay in creation order when a status changes", () => {
  const before = [agent("old", "idle", { createdAt: 1 }), agent("new", "working", { createdAt: 2 })];
  const after = [agent("old", "needsYou", { createdAt: 1, updatedAt: 9 }), agent("new", "done", { createdAt: 2 })];
  assert.deepEqual(ids(allSessions(before)), ["new", "old"]);
  assert.deepEqual(ids(allSessions(after)), ["new", "old"]);
});

test("a new session goes first", () => {
  const list = [agent("a", "idle", { createdAt: 1 }), agent("b", "idle", { createdAt: 2 })];
  assert.deepEqual(ids(allSessions([...list, agent("c", "idle", { createdAt: 3 })])), ["c", "b", "a"]);
});

test("up next ranks needs you then failed then done", () => {
  const list = [
    agent("done", "done", { updatedAt: 1 }),
    agent("failed", "done", { failed: true, updatedAt: 2 }),
    agent("needs", "needsYou", { updatedAt: 3 }),
  ];
  assert.deepEqual(ids(upNext(list)), ["needs", "failed", "done"]);
});

test("up next breaks ties by oldest transition", () => {
  const list = [agent("late", "needsYou", { updatedAt: 20 }), agent("early", "needsYou", { updatedAt: 10 })];
  assert.deepEqual(ids(upNext(list)), ["early", "late"]);
});

test("working idle and not attached sessions are not up next", () => {
  const list = [agent("w", "working"), agent("i", "idle"), agent("x", "needsYou", { attached: false })];
  assert.deepEqual(upNext(list), []);
});

test("summary lists urgency order and omits zeros", () => {
  const list = [
    agent("w1", "working"),
    agent("d1", "done"),
    agent("n1", "needsYou"),
    agent("w2", "working"),
    agent("f1", "done", { failed: true }),
    agent("d2", "done"),
    agent("w3", "working"),
    agent("i1", "idle"),
  ];
  assert.equal(summary(list), "1 needs you · 1 failed · 2 done · 3 working");
  assert.equal(summary([agent("d", "done"), agent("w", "working")]), "1 done · 1 working");
});

test("summary says need you for more than one", () => {
  assert.equal(summary([agent("a", "needsYou"), agent("b", "needsYou")]), "2 need you");
});

test("summary falls back to a session count", () => {
  assert.equal(summary([agent("a", "idle")]), "1 session");
  assert.equal(summary([agent("a", "idle"), agent("b", "working", { attached: false })]), "2 sessions");
});

test("summary is empty with no sessions", () => {
  assert.equal(summary([]), null);
});

test("the back badge counts needs you in other sessions only", () => {
  const list = [agent("here", "needsYou"), agent("a", "needsYou"), agent("b", "needsYou"), agent("c", "done")];
  assert.equal(needsYouElsewhere(list, "here"), 2);
  assert.equal(needsYouElsewhere(list, "a"), 2);
  assert.equal(needsYouElsewhere([agent("here", "needsYou")], "here"), 0);
  assert.equal(backLabel(0), "Back to sessions");
  assert.equal(backLabel(1), "Back to sessions, 1 needs you");
  assert.equal(backLabel(2), "Back to sessions, 2 need you");
});
