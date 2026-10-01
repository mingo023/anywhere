import { test } from "node:test";
import assert from "node:assert/strict";
import { outcome, read, serverTooOld } from "../src/connection.ts";

test("a removed or unknown phone stops as revoked", () => {
  assert.equal(outcome(4401, "not_paired"), "revoked");
  assert.equal(outcome(4401, "revoked"), "revoked");
  assert.equal(outcome(1000, "not_paired"), "revoked");
});

test("a version mismatch names the side to update", () => {
  assert.equal(outcome(4426, "server_too_old"), "updateMac");
  assert.equal(outcome(4426, "client_too_old"), "updatePhone");
});

test("anything else retries", () => {
  assert.equal(outcome(1006), "retry");
  assert.equal(outcome(1008, "rate_limited"), "retry");
});

test("a hello.ok below the phone's minimum means the Mac is too old", () => {
  assert.ok(serverTooOld({ protocolVersion: 2 }));
  assert.ok(serverTooOld({ protocolVersion: 3, protocol: { min: 1, max: 2 } }));
  assert.ok(!serverTooOld({ protocolVersion: 3 }));
  assert.ok(!serverTooOld({ protocolVersion: 3, protocol: { min: 3, max: 4 } }));
});

test("a hello reply that isn't JSON means the Mac needs updating", () => {
  assert.equal(read("<html>", false), "updateMac");
  assert.equal(read("null", false), "updateMac");
});

test("a Mac that calls the hello Malformed needs updating", () => {
  assert.equal(read(JSON.stringify({ type: "error", message: "Malformed message" }), false), "updateMac");
});

test("a hello.ok from a Mac below the phone's minimum needs updating", () => {
  assert.equal(read(JSON.stringify({ type: "hello.ok", hostname: "mac", protocolVersion: 2 }), false), "updateMac");
});

test("once online, a frame that isn't JSON is dropped and a Malformed error passes through", () => {
  assert.equal(read("<html>", true), undefined);
  const malformed = { type: "error", message: "Malformed message" };
  assert.deepEqual(read(JSON.stringify(malformed), true), malformed);
});

test("a coded error before hello.ok passes through", () => {
  const revoked = { type: "error", code: "not_paired", message: "Not paired" };
  assert.deepEqual(read(JSON.stringify(revoked), false), revoked);
});
