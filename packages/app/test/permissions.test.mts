import { test } from "node:test";
import assert from "node:assert/strict";
import type { PermissionRequest } from "@pocket/protocol";
import { STALE, acked, add, empty, failed, header, pending, resolved, sent } from "../src/permissions.ts";

const request = (requestId: string, agentId = "a1", toolName = "Bash") =>
  ({ requestId, agentId, toolName, detail: { kind: "shell", command: "ls" } }) as PermissionRequest;

const ids = (list: PermissionRequest[]) => list.map((r) => r.requestId);

const two = add(add(empty, request("perm-1")), request("perm-2", "a2", "Edit"));

test("a second request keeps the first", () => {
  assert.deepEqual(ids(pending(two)), ["perm-1", "perm-2"]);
});

test("resolving one keeps the rest", () => {
  assert.deepEqual(ids(pending(sent(two, "c7", "perm-1"))), ["perm-2"]);
});

test("an ack drops only its request", () => {
  const p = acked(sent(two, "c7", "perm-1"), "c7");
  assert.deepEqual(Object.keys(p.open), ["perm-2"]);
  assert.deepEqual(p.sending, {});
});

test("a stale error drops silently and another error keeps the page", () => {
  const answering = sent(two, "c7", "perm-1");
  assert.deepEqual(Object.keys(failed(answering, "c7", STALE).open), ["perm-2"]);
  assert.deepEqual(ids(pending(failed(answering, "c7", "socket closed"))), ["perm-1", "perm-2"]);
});

test("resolved from another client drops it", () => {
  assert.deepEqual(ids(pending(resolved(two, "perm-2"))), ["perm-1"]);
});

test("the header counts the open requests", () => {
  assert.equal(header(two), "Bash · 1 of 2");
  assert.equal(header(resolved(two, "perm-2")), "Bash");
  assert.equal(header(empty), undefined);
});

test("pending keeps arrival order and filters by agent", () => {
  const p = add(add(add(empty, request("perm-10")), request("perm-9")), request("perm-11", "a2"));
  assert.deepEqual(ids(pending(p)), ["perm-10", "perm-9", "perm-11"]);
  assert.deepEqual(ids(pending(p, "a2")), ["perm-11"]);
});
