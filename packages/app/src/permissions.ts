import type { PermissionRequest } from "@pocket/protocol";

/** pocketd's answer to a resolve for a request someone else already answered. */
export const STALE = "Permission request is no longer open";

export type Permissions = {
  /** By request id, in arrival order: request ids are long non-integer strings, so insertion order holds. */
  open: Readonly<Record<string, PermissionRequest>>;
  /** Client message id → request id, until pocketd acks the resolve. */
  sending: Readonly<Record<string, string>>;
};

export const empty: Permissions = { open: {}, sending: {} };

function without<T>(record: Readonly<Record<string, T>>, key: string): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([k]) => k !== key));
}

export function add(p: Permissions, r: PermissionRequest): Permissions {
  return { ...p, open: { ...p.open, [r.requestId]: r } };
}

export function sent(p: Permissions, msgId: string, requestId: string): Permissions {
  return { ...p, sending: { ...p.sending, [msgId]: requestId } };
}

export function acked(p: Permissions, msgId: string): Permissions {
  const requestId = p.sending[msgId];
  if (requestId === undefined) return p;
  return { open: without(p.open, requestId), sending: without(p.sending, msgId) };
}

export function resolved(p: Permissions, requestId: string): Permissions {
  return { ...p, open: without(p.open, requestId) };
}

/** A stale request is gone for good; any other failure puts the request back on the sheet. */
export function failed(p: Permissions, msgId: string, message: string): Permissions {
  const requestId = p.sending[msgId];
  if (requestId === undefined) return p;
  return { open: message === STALE ? without(p.open, requestId) : p.open, sending: without(p.sending, msgId) };
}

export function pending(p: Permissions, agentId?: string): PermissionRequest[] {
  const answering = new Set(Object.values(p.sending));
  return Object.values(p.open).filter((r) => !answering.has(r.requestId) && (agentId === undefined || r.agentId === agentId));
}

export function header(p: Permissions): string | undefined {
  const list = pending(p);
  const first = list[0];
  if (!first) return undefined;
  return list.length > 1 ? `${first.toolName} · 1 of ${list.length}` : first.toolName;
}
