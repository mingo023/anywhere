import type { ConnectionState } from "./client";

export const BACKOFF = { first: 250, cap: 16_000, stable: 30_000 } as const;
export const GRACE_MS = 4_000;
export const UNREACHABLE_MS = 30_000;
export const FIRST_CONNECT_MS = 10_000;
/** Matches pocketd's defaultHelloTimeout, after which it drops an unauthenticated socket anyway. */
export const HELLO_TIMEOUT_MS = 10_000;

export type Link = { state: ConnectionState; since: number; everOnline: boolean };

export type Banner =
  | { kind: "connecting"; text: string }
  | { kind: "reconnecting"; text: string }
  | { kind: "unreachable"; text: string };

export const noLink: Link = { state: "idle", since: 0, everOnline: false };

/** `since` is when the link last went down, so retries flipping connecting ⇄ offline don't restart the clock. */
export function track(link: Link, state: ConnectionState, now: number): Link {
  switch (state) {
    case "online":
      return { state, since: now, everOnline: true };
    case "idle":
      return { state, since: now, everOnline: false };
    case "rejected":
      return { ...link, state, since: now };
    case "connecting":
    case "offline":
      return link.state === "connecting" || link.state === "offline" ? { ...link, state } : { ...link, state, since: now };
  }
}

export function banner(link: Link, now: number, host: string): Banner | null {
  if (link.state !== "connecting" && link.state !== "offline") return null;
  const down = now - link.since;
  const unreachable: Banner = {
    kind: "unreachable",
    text: `Can't reach ${host}. Is your Mac awake (lid open) and on Tailscale?`,
  };
  if (!link.everOnline) return down >= FIRST_CONNECT_MS ? unreachable : { kind: "connecting", text: `Connecting to ${host}…` };
  if (down >= UNREACHABLE_MS) return unreachable;
  return down >= GRACE_MS ? { kind: "reconnecting", text: "Reconnecting…" } : null;
}

export function retry(link: Link, now: number): Link {
  return link.everOnline ? link : { ...link, since: now };
}

export function nextBackoff(ms: number): number {
  return Math.min(ms * 2, BACKOFF.cap);
}

export type Net = { isConnected: boolean | null; type: string };

/** The first report is the baseline; after it, coming online or switching interface means the socket may be dead. */
export function shouldRedial(prev: Net | undefined, next: Net): boolean {
  if (!prev || next.isConnected !== true) return false;
  return prev.isConnected !== true || prev.type !== next.type;
}
