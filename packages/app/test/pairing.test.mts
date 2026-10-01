import { test } from "node:test";
import assert from "node:assert/strict";
import { PairError, isCode, isHost, pair, pairErrorText, pairFailure, parsePairURL, wsURL } from "../src/pairing.ts";

const code = "q3xYq3xYq3xYq3xYq3xY_-";

test("a pair link gives the host, code and Mac name", () => {
  const url = `anywhere://pair?v=1&h=100.77.122.82%3A4517&c=${code}&n=Mingo%27s+MacBook+Pro`;
  assert.deepEqual(parsePairURL(url), { host: "100.77.122.82:4517", code, name: "Mingo's MacBook Pro" });
});

test("a pair link without a name is named after its host", () => {
  assert.equal(parsePairURL(`anywhere://pair?v=1&h=127.0.0.1:4517&c=${code}`)?.name, "127.0.0.1:4517");
});

test("pair links from another version or with a bad code or host are refused", () => {
  for (const url of [
    `anywhere://pair?v=2&h=127.0.0.1:4517&c=${code}`,
    `anywhere://pair?v=1&h=127.0.0.1:4517&c=short`,
    `anywhere://pair?v=1&h=127.0.0.1&c=${code}`,
    `anywhere://pair?v=1&h=127.0.0.1:99999&c=${code}`,
    `anywhere://pair?v=1&h=%E0%A4%A&c=${code}`,
    `https://example.com/pair?v=1&h=127.0.0.1:4517&c=${code}`,
  ]) {
    assert.equal(parsePairURL(url), null, url);
  }
});

test("a typed code must be the 22 characters from the Mac", () => {
  assert.ok(isCode(code));
  assert.ok(!isCode(code.slice(1)));
  assert.ok(!isCode(`${code.slice(1)}!`));
});

test("a Mac address needs a port", () => {
  assert.ok(isHost("mac-mini.tail1234.ts.net:4517"));
  assert.ok(isHost("[fd7a:115c:a1e0::1]:4517"));
  assert.ok(!isHost("mac-mini.tail1234.ts.net"));
  assert.ok(!isHost("ws://127.0.0.1:4517"));
});

test("loopback and tailnet hosts use plain ws, others wss", () => {
  assert.equal(wsURL("127.0.0.1:4517"), "ws://127.0.0.1:4517");
  assert.equal(wsURL("100.77.122.82:4517"), "ws://100.77.122.82:4517");
  assert.equal(wsURL("[fd7a:115c:a1e0::1]:4517"), "ws://[fd7a:115c:a1e0::1]:4517");
  assert.equal(wsURL("mac-mini.tail1234.ts.net:4517"), "ws://mac-mini.tail1234.ts.net:4517");
  assert.equal(wsURL("100.128.0.1:4517"), "wss://100.128.0.1:4517");
  assert.equal(wsURL("example.com:443"), "wss://example.com:443");
  assert.equal(wsURL("127.evil.example:1"), "wss://127.evil.example:1");
  assert.equal(wsURL("100.64.evil.example:1"), "wss://100.64.evil.example:1");
});

test("an address with a leading-zero octet is not the tailnet", () => {
  assert.equal(wsURL("100.064.0.1:4517"), "wss://100.064.0.1:4517");
  assert.equal(wsURL("127.0.0.01:4517"), "wss://127.0.0.01:4517");
  assert.equal(wsURL("100.64.0.0:4517"), "ws://100.64.0.0:4517");
});

test("a pair reply that isn't JSON fails the pairing", async () => {
  const sockets: FakeSocket[] = [];
  class FakeSocket {
    onmessage?: (event: { data: string }) => void;
    onclose?: () => void;
    constructor() {
      sockets.push(this);
    }
    close() {
      this.onclose?.();
    }
  }
  const real = globalThis.WebSocket;
  globalThis.WebSocket = FakeSocket as unknown as typeof WebSocket;
  try {
    const pending = pair("127.0.0.1:4517", { code, name: "iPhone", platform: "ios" });
    sockets[0]!.onmessage!({ data: "<html>" });
    await assert.rejects(pending, (e) => e instanceof PairError && e.code === "pair_failed");
  } finally {
    globalThis.WebSocket = real;
  }
});

test("the pairing socket closes once paired", async () => {
  const sockets: FakeSocket[] = [];
  class FakeSocket {
    onmessage?: (event: { data: string }) => void;
    onclose?: () => void;
    closed = false;
    constructor() {
      sockets.push(this);
    }
    close() {
      this.closed = true;
      this.onclose?.();
    }
  }
  const real = globalThis.WebSocket;
  globalThis.WebSocket = FakeSocket as unknown as typeof WebSocket;
  try {
    const pending = pair("127.0.0.1:4517", { code, name: "iPhone", platform: "ios" });
    sockets[0]!.onmessage!({ data: JSON.stringify({ type: "pair.ok", id: "p1", deviceId: "d1", token: "t" }) });
    assert.deepEqual(await pending, { deviceId: "d1", token: "t" });
    assert.ok(sockets[0]!.closed);
  } finally {
    globalThis.WebSocket = real;
  }
});

test("a Mac too old to know pair says Malformed message", () => {
  assert.equal(pairFailure({ message: "Malformed message" }), "server_too_old");
  assert.equal(pairFailure({ message: "This code doesn't match. Check it on your Mac.", code: "pair_invalid" }), "pair_invalid");
  assert.equal(pairFailure({ message: "Pairing failed. Make a new code on your Mac." }), "pair_failed");
});

test("every pairing failure reads as a next step", () => {
  assert.equal(pairErrorText("pair_expired", "h:1"), "Code expired. Make a new one on your Mac.");
  assert.equal(pairErrorText("pair_used", "h:1"), "This code was already used.");
  assert.equal(pairErrorText("pair_invalid", "h:1"), "This code doesn't match. Check it on your Mac.");
  assert.equal(pairErrorText("pair_locked", "h:1"), "Too many tries. Make a new code on your Mac in a minute.");
  assert.equal(pairErrorText("rate_limited", "h:1"), "Too many tries. Make a new code on your Mac in a minute.");
  assert.equal(pairErrorText("unreachable", "h:1"), "Can't reach h:1. Is Tailscale on?");
  assert.equal(pairErrorText("server_too_old", "h:1"), "Update Pocket on your Mac.");
  assert.equal(pairErrorText("client_too_old", "h:1"), "Update Pocket on this phone.");
  assert.equal(pairErrorText("pair_failed", "h:1"), "Pairing failed. Make a new code on your Mac.");
});
