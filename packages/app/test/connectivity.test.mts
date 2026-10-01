import { test } from "node:test";
import assert from "node:assert/strict";
import {
  BACKOFF,
  banner,
  nextBackoff,
  shouldRedial,
  noLink,
  retry,
  track,
  type Link,
} from "../src/connectivity.ts";

const host = "mac.local";
const lid = "Can't reach mac.local. Is your Mac awake (lid open) and on Tailscale?";
const dropped: Link = { state: "offline", since: 1_000, everOnline: true };
const text = (link: Link, now: number) => banner(link, now, host)?.text ?? null;

test("a blip under four seconds shows nothing", () => {
  assert.equal(text(dropped, 1_000 + 3_999), null);
  assert.equal(text({ ...dropped, state: "connecting" }, 1_000 + 3_999), null);
});

test("a drop shows reconnecting from four seconds", () => {
  assert.equal(text(dropped, 1_000 + 4_000), "Reconnecting…");
  assert.equal(text({ ...dropped, state: "connecting" }, 1_000 + 29_999), "Reconnecting…");
});

test("a long drop names the lid and tailscale", () => {
  assert.deepEqual(banner(dropped, 1_000 + 30_000, host), { kind: "unreachable", text: lid });
});

test("a first connect shows connecting then unreachable after ten seconds", () => {
  const first: Link = { state: "connecting", since: 0, everOnline: false };
  assert.deepEqual(banner(first, 0, host), { kind: "connecting", text: "Connecting to mac.local…" });
  assert.deepEqual(banner(first, 9_999, host), { kind: "connecting", text: "Connecting to mac.local…" });
  assert.equal(text({ ...first, state: "offline" }, 10_000), lid);
});

test("a retry restarts only the first-connect clock", () => {
  const failed: Link = { state: "offline", since: 0, everOnline: false };
  assert.equal(banner(retry(failed, 20_000), 20_000, host)?.kind, "connecting");
  assert.deepEqual(retry(dropped, 20_000), dropped);
});

test("online, signed out and rejected show nothing", () => {
  assert.equal(text({ state: "online", since: 0, everOnline: true }, 60_000), null);
  assert.equal(text(noLink, 60_000), null);
  assert.equal(text({ state: "rejected", since: 0, everOnline: false }, 60_000), null);
});

test("retries keep the time the link went down", () => {
  let link = track(noLink, "connecting", 100);
  link = track(link, "online", 200);
  link = track(link, "offline", 300);
  link = track(link, "connecting", 900);
  link = track(link, "offline", 1_500);
  assert.deepEqual(link, { state: "offline", since: 300, everOnline: true });
  assert.deepEqual(track(link, "idle", 2_000), { state: "idle", since: 2_000, everOnline: false });
});

test("backoff doubles from 250 ms to a 16 s cap", () => {
  const steps = [BACKOFF.first];
  while (steps.length < 9) steps.push(nextBackoff(steps[steps.length - 1]!));
  assert.deepEqual(steps, [250, 500, 1_000, 2_000, 4_000, 8_000, 16_000, 16_000, 16_000]);
});

test("coming online or switching network redials", () => {
  assert.equal(shouldRedial({ isConnected: false, type: "none" }, { isConnected: true, type: "wifi" }), true);
  assert.equal(shouldRedial({ isConnected: true, type: "wifi" }, { isConnected: true, type: "cellular" }), true);
});

test("an unchanged network does not redial", () => {
  assert.equal(shouldRedial(undefined, { isConnected: true, type: "wifi" }), false);
  assert.equal(shouldRedial({ isConnected: true, type: "wifi" }, { isConnected: true, type: "wifi" }), false);
  assert.equal(shouldRedial({ isConnected: true, type: "wifi" }, { isConnected: false, type: "none" }), false);
});
