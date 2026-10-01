import { test } from "node:test";
import assert from "node:assert/strict";
import type { Project, ProviderInfo } from "@pocket/protocol";
import { ACCESSES, LOCKED, RECEIPT_TTL_MS, UNCONFIRMED, blocker, canPlan, clamp, failure, installed, locked, open, pickProvider, reconnect, remember, spec, worktreeName, type Draft, type Launch, type Picks } from "../src/launch.ts";

const project = (path: string): Project => ({ path, name: path.slice(1), worktrees: [{ name: path.slice(1), path, branch: "main", isMain: true }] });
const providers: ProviderInfo[] = [
  { id: "claude", available: true, efforts: [], plan: true },
  { id: "codex", available: true, efforts: [], plan: false },
];
const draft: Draft = { project: "/a", worktree: "/a/w", provider: "claude", access: "edits", plan: true };

test("a spec carries the chips and the trimmed prompt", () => {
  assert.deepEqual(spec(draft, "fix-ci", "  Fix CI  "), {
    project: "/a",
    checkout: { worktree: "/a/w" },
    provider: "claude",
    access: "edits",
    plan: true,
    prompt: "Fix CI",
  });
});

test("never sends base from the phone", () => {
  assert.deepEqual(spec({ ...draft, worktree: undefined }, "fix-ci", "Fix CI").checkout, { new: { name: "fix-ci" } });
});

test("access above the Mac's ceiling is locked and not restored", () => {
  assert.deepEqual(ACCESSES.filter((a) => locked(a, "ask")), ["edits", "auto"]);
  assert.deepEqual(ACCESSES.filter((a) => locked(a, "edits")), ["auto"]);
  assert.deepEqual(ACCESSES.filter((a) => locked(a, "auto")), []);
  assert.deepEqual(ACCESSES.filter((a) => locked(a, "")), ["edits", "auto"]);
  assert.equal(open(remember({ projects: {} }, { ...draft, access: "auto" }), [project("/a")], "edits", providers)?.access, "ask");
});

test("a chip left above a lowered ceiling drops to the ceiling", () => {
  assert.equal(clamp({ ...draft, access: "auto" }, "edits").access, "edits");
  assert.equal(clamp({ ...draft, access: "edits" }, "ask").access, "ask");
  assert.equal(clamp({ ...draft, access: "edits" }, "").access, "ask");
  assert.equal(clamp(draft, "auto"), draft);
});

test("disables plan for codex", () => {
  assert.equal(canPlan(providers, "codex"), false);
  assert.equal(pickProvider(draft, providers, "codex").plan, false);
  assert.equal(pickProvider(draft, providers, "claude").plan, true);
});

test("maps every error code to its copy", () => {
  assert.deepEqual(failure("invalid_name", "x"), { message: "Use letters, digits, - _ or .", name: true });
  assert.deepEqual(failure("worktree_exists", "x"), { message: "A worktree or branch with this name already exists", name: true });
  assert.deepEqual(failure("folder_not_trusted", "x"), { message: "Trust this folder in Claude on your Mac first" });
  assert.deepEqual(failure("access_not_allowed", "x"), { message: LOCKED, refetch: true });
  assert.deepEqual(failure("spawn_failed", "Setup exited 1", "npm ERR!"), { message: "Setup exited 1", detail: "npm ERR!" });
  assert.deepEqual(failure("unknown_project", "Unknown project"), { message: "Unknown project" });
  assert.deepEqual(blocker([], [], []), { title: "Update Pocket on your Mac." });
  assert.deepEqual(blocker(["launch.v1"], [], []), { title: "Add a project on your Mac first." });
  const none = [{ id: "claude", available: false, efforts: [], plan: true }];
  assert.deepEqual(blocker(["launch.v1"], [project("/a")], none), { title: "No agents available", body: "Install Claude Code or Codex, then reopen." });
  assert.equal(blocker(["launch.v1"], [project("/a")], providers), undefined);
});

test("a Mac that calls a create Malformed is too old for it", () => {
  assert.deepEqual(failure(undefined, "Malformed message"), { message: "Update Pocket on your Mac." });
});

test("slugs the first four words of the prompt", () => {
  assert.equal(worktreeName("Fix the flaky CI test on main", 0, []), "fix-the-flaky-ci");
  assert.equal(worktreeName("Fix the flaky CI", 0, ["fix-the-flaky-ci", "Fix-The-Flaky-CI-2/wip"]), "fix-the-flaky-ci-3");
  assert.equal(worktreeName("  ", 9, []), "calm-heron");
  assert.equal(worktreeName("", 9, ["calm-heron"]), "calm-maple");
});

test("remembers picks per project but not plan", () => {
  let picks: Picks = { projects: {} };
  picks = remember(picks, { project: "/a", worktree: "/a", provider: "codex", access: "auto", plan: false });
  picks = remember(picks, { project: "/b", worktree: "/b", provider: "claude", access: "edits", plan: true });
  const both = [project("/a"), project("/b")];
  assert.deepEqual(open(picks, both, "auto", providers), { project: "/b", worktree: "/b", provider: "claude", access: "edits", plan: false });
  assert.deepEqual(open({ ...picks, last: "/a" }, both, "auto", providers), { project: "/a", worktree: "/a", provider: "codex", access: "auto", plan: false });
  assert.deepEqual(open({ projects: {} }, both, "auto", providers), { project: "/a", worktree: "/a", provider: "claude", access: "ask", plan: false });
  assert.equal(open(picks, [], "auto", providers), undefined);
});

test("a pick restores its model and effort", () => {
  const picks = remember({ projects: {} }, { ...draft, model: "opus", effort: "high" });
  const opened = open(picks, [project("/a")], "auto", providers);
  assert.equal(opened?.model, "opus");
  assert.equal(opened?.effort, "high");
  assert.deepEqual(spec(opened!, "fix-ci", "Fix CI").model, "opus");
  assert.deepEqual(spec(opened!, "fix-ci", "Fix CI").effort, "high");
});

test("a pick stored before model and effort still opens", () => {
  const old = { last: "/a", projects: { "/a": { provider: "codex", access: "edits" } } } as Picks;
  assert.deepEqual(open(old, [project("/a")], "auto", providers), { project: "/a", worktree: "/a", provider: "codex", access: "edits", plan: false });
});

test("another agent does not inherit the model and effort", () => {
  const picked = pickProvider({ ...draft, model: "opus", effort: "high" }, providers, "codex");
  assert.equal(picked.model, undefined);
  assert.equal(picked.effort, undefined);
  const picks = remember({ projects: {} }, { ...draft, model: "opus", effort: "high" });
  const noClaude = [{ ...providers[0]!, available: false }, providers[1]!];
  assert.equal(open(picks, [project("/a")], "auto", noClaude)?.model, undefined);
});

test("falls back to the first installed agent when the remembered one is gone", () => {
  const picks = remember({ projects: {} }, { ...draft, provider: "claude" });
  const noClaude = [{ ...providers[0]!, available: false }, providers[1]!];
  assert.equal(open(picks, [project("/a")], "auto", noClaude)?.provider, "codex");
  assert.equal(open({ projects: {} }, [project("/a")], "auto", noClaude)?.provider, "codex");
});

test("an agent that isn't installed can't be sent", () => {
  const noClaude = [{ ...providers[0]!, available: false }, providers[1]!];
  assert.equal(installed(noClaude, "claude"), false);
  assert.equal(installed(noClaude, "codex"), true);
  assert.equal(installed([], "claude"), false);
});

const sent: Launch = { requestId: "r1", id: "c4", sentAt: 1_000, spec: spec(draft, "fix-ci", "Fix CI") };

test("a pending create is resent under its own requestId", () => {
  assert.deepEqual(reconnect(sent, 1_000 + RECEIPT_TTL_MS - 1), { launch: sent, send: { type: "agent.create", requestId: "r1", spec: sent.spec } });
});

test("a settled create is not resent", () => {
  assert.equal(reconnect({ ...sent, agentId: "a1" }, 1_000), undefined);
  assert.equal(reconnect({ ...sent, failure: { message: "Unknown project" } }, 1_000), undefined);
  assert.equal(reconnect(undefined, 1_000), undefined);
});

test("a create older than pocketd's receipts fails instead of being resent", () => {
  assert.deepEqual(reconnect(sent, 1_000 + RECEIPT_TTL_MS), { launch: { ...sent, failure: { message: UNCONFIRMED } } });
});
