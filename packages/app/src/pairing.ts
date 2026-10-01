import { PROTOCOL_MAX, PROTOCOL_MIN } from "@pocket/protocol/constants";
import { MALFORMED } from "./connection.ts";

export type PairLink = { host: string; code: string; name: string };
export type PairRequest = { code: string; name: string; platform: "ios" | "android" };

const CODE = /^[A-Za-z0-9_-]{22}$/;
const HOST = /^(\[[0-9A-Fa-f:]+\]|[A-Za-z0-9.-]+):(\d{1,5})$/;
const PREFIX = "anywhere://pair?";
const PAIR_TIMEOUT = 10_000;

export function isCode(code: string): boolean {
  return CODE.test(code);
}

export function isHost(host: string): boolean {
  const port = HOST.exec(host)?.[2];
  return port !== undefined && Number(port) >= 1 && Number(port) <= 65535;
}

// Parsed by hand: React Native's URL has no working searchParams.
function query(search: string): Map<string, string> | null {
  const params = new Map<string, string>();
  try {
    for (const part of search.split("&")) {
      const at = part.indexOf("=");
      if (at > 0) params.set(part.slice(0, at), decodeURIComponent(part.slice(at + 1).replace(/\+/g, " ")));
    }
  } catch {
    return null;
  }
  return params;
}

export function parsePairURL(url: string): PairLink | null {
  if (!url.startsWith(PREFIX)) return null;
  const params = query(url.slice(PREFIX.length));
  const host = params?.get("h") ?? "";
  const code = params?.get("c") ?? "";
  if (params?.get("v") !== "1" || !isCode(code) || !isHost(host)) return null;
  return { host, code, name: params.get("n") || host };
}

function ipv4(hostname: string): number[] | null {
  const octets = hostname.split(".");
  return octets.length === 4 && octets.every((o) => /^(0|[1-9]\d{0,2})$/.test(o) && Number(o) <= 255) ? octets.map(Number) : null;
}

function onTailnet(hostname: string): boolean {
  const [a, b] = ipv4(hostname) ?? [];
  return (
    hostname === "localhost" ||
    a === 127 ||
    (a === 100 && b !== undefined && b >= 64 && b <= 127) ||
    hostname === "[::1]" ||
    hostname.toLowerCase().startsWith("[fd7a:115c:a1e0:") ||
    hostname.endsWith(".ts.net")
  );
}

/** wsURL uses plain ws where WireGuard already encrypts: loopback and the tailnet. */
export function wsURL(host: string): string {
  const hostname = host.slice(0, host.lastIndexOf(":"));
  return `${onTailnet(hostname) ? "ws" : "wss"}://${host}`;
}

export function pairFailure(error: { code?: string; message: string }): string {
  if (error.code) return error.code;
  return error.message === MALFORMED ? "server_too_old" : "pair_failed";
}

const TOO_MANY = "Too many tries. Make a new code on your Mac in a minute.";

const PAIR_TEXT: Record<string, string> = {
  pair_expired: "Code expired. Make a new one on your Mac.",
  pair_used: "This code was already used.",
  pair_invalid: "This code doesn't match. Check it on your Mac.",
  pair_locked: TOO_MANY,
  rate_limited: TOO_MANY,
  server_too_old: "Update Pocket on your Mac.",
  client_too_old: "Update Pocket on this phone.",
};

export function pairErrorText(code: string, host: string): string {
  if (code === "unreachable") return `Can't reach ${host}. Is Tailscale on?`;
  return PAIR_TEXT[code] ?? "Pairing failed. Make a new code on your Mac.";
}

export class PairError extends Error {
  readonly code: string;
  constructor(code: string) {
    super(code);
    this.code = code;
  }
}

export function pair(host: string, req: PairRequest): Promise<{ deviceId: string; token: string }> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsURL(host));
    let failure = "unreachable";
    const timer = setTimeout(() => ws.close(), PAIR_TIMEOUT);
    ws.onopen = () => {
      ws.send(JSON.stringify({ type: "pair", id: "p1", ...req, protocol: { min: PROTOCOL_MIN, max: PROTOCOL_MAX } }));
    };
    ws.onmessage = (event) => {
      let msg: { type: string; deviceId: string; token: string; code?: string; message: string };
      try {
        msg = JSON.parse(String(event.data));
      } catch {
        failure = "pair_failed";
        ws.close();
        return;
      }
      if (msg.type === "pair.ok") {
        clearTimeout(timer);
        resolve({ deviceId: msg.deviceId, token: msg.token });
        ws.close();
      } else if (msg.type === "error") {
        failure = pairFailure(msg);
        ws.close();
      }
    };
    ws.onclose = () => {
      clearTimeout(timer);
      reject(new PairError(failure));
    };
  });
}
