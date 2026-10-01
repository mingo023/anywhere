import { PROTOCOL_MIN } from "@pocket/protocol/constants";
import type { ServerMessage } from "@pocket/protocol";

export const MALFORMED = "Malformed message";

export type Outcome = "retry" | "revoked" | "updateMac" | "updatePhone";

export function outcome(closeCode: number, errorCode?: string): Outcome {
  if (closeCode === 4401 || errorCode === "not_paired" || errorCode === "revoked") return "revoked";
  if (closeCode === 4426) return errorCode === "server_too_old" ? "updateMac" : "updatePhone";
  return "retry";
}

export function serverTooOld(ok: { protocolVersion: number; protocol?: { max: number } }): boolean {
  return (ok.protocol?.max ?? ok.protocolVersion) < PROTOCOL_MIN;
}

/** Before hello.ok, a reply pocketd can't have sent or a refusal of the hello itself means the Mac predates this phone. */
export function read(data: string, online: boolean): ServerMessage | "updateMac" | undefined {
  let msg: ServerMessage | null;
  try {
    msg = JSON.parse(data);
  } catch {
    msg = null;
  }
  if (typeof msg !== "object" || msg === null) return online ? undefined : "updateMac";
  if (msg.type === "hello.ok" && serverTooOld(msg)) return "updateMac";
  if (!online && msg.type === "error" && !msg.code && msg.message === MALFORMED) return "updateMac";
  return msg;
}
