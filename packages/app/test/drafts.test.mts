import { test } from "node:test";
import assert from "node:assert/strict";
import { createDrafts } from "../src/drafts.ts";

test("a draft reads back what was set", () => {
  const drafts = createDrafts();
  drafts.set("a", "fix the login");
  assert.equal(drafts.get("a"), "fix the login");
});

test("drafts are kept per session", () => {
  const drafts = createDrafts();
  drafts.set("a", "first");
  drafts.set("b", "second");
  assert.deepEqual([drafts.get("a"), drafts.get("b")], ["first", "second"]);
});

test("sending clears only that draft", () => {
  const drafts = createDrafts();
  drafts.set("a", "first");
  drafts.set("b", "second");
  drafts.clear("a");
  assert.deepEqual([drafts.get("a"), drafts.get("b")], ["", "second"]);
});

test("an emptied draft reads empty", () => {
  const drafts = createDrafts();
  drafts.set("a", "typo");
  drafts.set("a", "");
  assert.equal(drafts.get("a"), "");
});
