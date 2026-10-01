export type KV = {
  getItemAsync(key: string): Promise<string | null>;
  setItemAsync(key: string, value: string): Promise<void>;
  deleteItemAsync(key: string): Promise<void>;
};

export type Creds = { host: string; token: string; deviceId?: string; macName?: string };

const HOST = "pocket.macHost";
const TOKEN = "pocket.deviceToken";
const DEVICE_ID = "pocket.deviceId";
const MAC_NAME = "pocket.macName";
const CLIENT_ID = "pocket.clientId";

/* Host and token used to be saved with iOS's default accessibility, which an
   update keeps, so they move to new keys saved with the KV's option. The new
   key is written before the old one goes, and never over a newer pairing. */
const LEGACY = [
  ["pocket.host", HOST],
  ["pocket.token", TOKEN],
] as const;

async function move(kv: KV, from: string, to: string): Promise<void> {
  const value = await kv.getItemAsync(from);
  if (value === null) return;
  if ((await kv.getItemAsync(to)) === null) await kv.setItemAsync(to, value);
  await kv.deleteItemAsync(from);
}

async function put(kv: KV, key: string, value: string | undefined): Promise<void> {
  if (value === undefined) await kv.deleteItemAsync(key);
  else await kv.setItemAsync(key, value);
}

export async function load(kv: KV): Promise<Creds | null> {
  for (const [from, to] of LEGACY) await move(kv, from, to);
  const host = await kv.getItemAsync(HOST);
  const token = await kv.getItemAsync(TOKEN);
  if (!host || !token) return null;
  const creds: Creds = { host, token };
  const deviceId = await kv.getItemAsync(DEVICE_ID);
  const macName = await kv.getItemAsync(MAC_NAME);
  if (deviceId) creds.deviceId = deviceId;
  if (macName) creds.macName = macName;
  return creds;
}

export async function save(kv: KV, c: Creds): Promise<void> {
  await kv.setItemAsync(HOST, c.host);
  await kv.setItemAsync(TOKEN, c.token);
  await put(kv, DEVICE_ID, c.deviceId);
  await put(kv, MAC_NAME, c.macName);
}

export function savedHost(kv: KV): Promise<string | null> {
  return kv.getItemAsync(HOST);
}

/** The host stays to prefill EnterCode. */
export async function wipe(kv: KV): Promise<void> {
  await kv.deleteItemAsync(TOKEN);
  await kv.deleteItemAsync(DEVICE_ID);
  await kv.deleteItemAsync(MAC_NAME);
}

export async function clientId(kv: KV, os: string): Promise<string> {
  const saved = await kv.getItemAsync(CLIENT_ID);
  if (saved) return saved;
  const random = Math.random().toString(36).slice(2, 10).padEnd(8, "0");
  const id = `${os}-${Date.now().toString(36)}-${random}`;
  await kv.setItemAsync(CLIENT_ID, id);
  return id;
}

/** Runs keychain jobs one at a time, so a wipe still in flight can't delete what a later save wrote. */
export function serially(): <T>(job: () => Promise<T>) => Promise<T> {
  let tail: Promise<unknown> = Promise.resolve();
  return (job) => {
    const run = tail.then(job);
    tail = run.catch(() => {});
    return run;
  };
}
