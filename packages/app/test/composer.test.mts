import { test } from "node:test";
import assert from "node:assert/strict";
import { QUEUES, composerAction, composerLabel, sendDisabled } from "../src/composer.ts";

test("an idle agent always gets send", () => {
  assert.equal(composerAction(false, "fix it"), "send");
  assert.equal(composerAction(false, "  "), "send");
});

test("a working agent with text gets queue and without text gets stop", () => {
  assert.equal(composerAction(true, "also add tests"), "queue");
  assert.equal(composerAction(true, "  "), "stop");
});

test("queue reads Queue only for a provider that step 0 showed queues", () => {
  for (const provider of ["claude", "codex", "gemini"]) {
    assert.equal(composerLabel("send", provider), "Send");
    assert.equal(composerLabel("queue", provider), QUEUES[provider] ? "Queue" : "Send");
    assert.equal(composerLabel("stop", provider), "Stop");
  }
});

test("send is disabled only for blank text on an idle agent", () => {
  assert.equal(sendDisabled(false, " \n"), true);
  assert.equal(sendDisabled(false, "go"), false);
  assert.equal(sendDisabled(true, ""), false);
});
