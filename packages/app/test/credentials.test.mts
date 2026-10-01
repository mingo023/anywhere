import { test } from "node:test";
import assert from "node:assert/strict";
import { clientId, load, save, savedHost, serially, wipe, type KV } from "../src/credentials.ts";

function store(seed: Record<string, string> = {}) {
  const items = new Map(Object.entries(seed));
  const writes: string[] = [];
  const kv: KV = {
    getItemAsync: async (key) => items.get(key) ?? null,
    setItemAsync: async (key, value) => {
      writes.push(`set ${key}`);
      items.set(key, value);
    },
    deleteItemAsync: async (key) => {
      writes.push(`delete ${key}`);
      items.delete(key);
    },
  };
  return { kv, writes };
}

test("saved credentials load back", async () => {
  const { kv } = store();
  await save(kv, { host: "100.77.122.82:4517", token: "t", deviceId: "3fa9", macName: "Mac mini" });
  assert.deepEqual(await load(kv), { host: "100.77.122.82:4517", token: "t", deviceId: "3fa9", macName: "Mac mini" });
});

test("pairing again drops the old device id", async () => {
  const { kv } = store();
  await save(kv, { host: "a:1", token: "t", deviceId: "3fa9", macName: "Mac mini" });
  await save(kv, { host: "b:1", token: "u" });
  assert.deepEqual(await load(kv), { host: "b:1", token: "u" });
});

test("without a token there is nothing to load", async () => {
  assert.equal(await load(store({ "pocket.host": "a:1" }).kv), null);
});

test("legacy host and token move to new keys once, then still load", async () => {
  const { kv, writes } = store({ "pocket.host": "a:1", "pocket.token": "legacy" });
  assert.deepEqual(await load(kv), { host: "a:1", token: "legacy" });
  assert.deepEqual(writes, ["set pocket.macHost", "delete pocket.host", "set pocket.deviceToken", "delete pocket.token"]);
  writes.length = 0;
  await load(kv);
  assert.deepEqual(writes, []);
});

test("a keychain failure while moving legacy credentials loses nothing", async () => {
  const { kv } = store({ "pocket.host": "a:1", "pocket.token": "legacy" });
  const set = kv.setItemAsync;
  kv.setItemAsync = async () => {
    throw new Error("keychain busy");
  };
  await assert.rejects(load(kv));
  kv.setItemAsync = set;
  assert.deepEqual(await load(kv), { host: "a:1", token: "legacy" });
});

test("a legacy key left behind never overwrites a newer pairing", async () => {
  const { kv } = store({ "pocket.token": "legacy" });
  await save(kv, { host: "b:1", token: "u" });
  assert.deepEqual(await load(kv), { host: "b:1", token: "u" });
});

test("the client id is made once and kept", async () => {
  const { kv } = store();
  const id = await clientId(kv, "ios");
  assert.match(id, /^ios-[0-9a-z]+-[0-9a-z]{8}$/);
  assert.equal(await clientId(kv, "ios"), id);
});

test("wiping a removed phone keeps its host and client id", async () => {
  const { kv } = store();
  await save(kv, { host: "a:1", token: "t", deviceId: "3fa9", macName: "Mac mini" });
  const id = await clientId(kv, "android");
  await wipe(kv);
  assert.equal(await load(kv), null);
  assert.equal(await savedHost(kv), "a:1");
  assert.equal(await clientId(kv, "android"), id);
});

test("a slow wipe queued before a new pairing never deletes its token", async () => {
  const { kv } = store();
  const slow: KV = {
    ...kv,
    deleteItemAsync: async (key) => {
      await new Promise((resolve) => setTimeout(resolve, 5));
      await kv.deleteItemAsync(key);
    },
  };
  await save(slow, { host: "a:1", token: "old" });
  const run = serially();
  const wiping = run(() => wipe(slow));
  await run(() => save(slow, { host: "b:1", token: "new" }));
  await wiping;
  assert.deepEqual(await load(slow), { host: "b:1", token: "new" });
});

test("a failed keychain job does not stop the next one", async () => {
  const run = serially();
  await assert.rejects(run(async () => {
    throw new Error("keychain busy");
  }));
  assert.equal(await run(async () => "next"), "next");
});
