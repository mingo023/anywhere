import { CAP_LAUNCH, CAP_REGISTRY, PROTOCOL_MAX, PROTOCOL_MIN } from "@pocket/protocol/constants";
import type { ClientMessage, ServerMessage } from "@pocket/protocol";
import { outcome, read, type Outcome } from "./connection";
import { BACKOFF, HELLO_TIMEOUT_MS, nextBackoff } from "./connectivity";

type Outbound = ClientMessage extends infer M ? (M extends { id: string } ? Omit<M, "id"> : never) : never;
export type ConnectionState = "idle" | "connecting" | "online" | "offline" | "rejected";

/** AppState and NetInfo often fire together; a second redial this soon would kill the dial the first one started. */
const REDIAL_SETTLE_MS = 1_000;

export class PocketClient {
  private ws?: WebSocket;
  private seq = 0;
  private backoff: number = BACKOFF.first;
  private retry?: ReturnType<typeof setTimeout>;
  private helloTimer?: ReturnType<typeof setTimeout>;
  private dialedAt = 0;
  private onlineAt?: number;
  private closed = false;

  constructor(
    private readonly url: string,
    private readonly token: string,
    private readonly clientId: string,
    private readonly onMessage: (msg: ServerMessage) => void,
    private readonly onState: (state: ConnectionState) => void,
    private readonly onEnd: (outcome: Exclude<Outcome, "retry">) => void,
  ) {}

  connect(): void {
    if (this.closed) return;
    this.drop();
    this.dialedAt = Date.now();
    this.onState("connecting");

    const ws = new WebSocket(this.url);
    this.ws = ws;
    let errorCode: string | undefined;
    this.helloTimer = setTimeout(() => this.lost(), HELLO_TIMEOUT_MS);

    ws.onopen = () => {
      ws.send(
        JSON.stringify({
          type: "hello",
          id: this.nextId(),
          token: this.token,
          clientId: this.clientId,
          protocolVersion: PROTOCOL_MAX,
          caps: ["pair.v1", "summary.v2", CAP_REGISTRY, CAP_LAUNCH],
          protocol: { min: PROTOCOL_MIN, max: PROTOCOL_MAX },
        }),
      );
    };

    ws.onmessage = (event) => {
      const msg = read(String(event.data), this.onlineAt !== undefined);
      if (msg === "updateMac") {
        this.lost("updateMac");
        return;
      }
      if (!msg) return;
      if (msg.type === "error") errorCode = msg.code;
      if (msg.type === "hello.ok") {
        clearTimeout(this.helloTimer);
        this.onlineAt = Date.now();
        this.onState("online");
      }
      this.onMessage(msg);
    };

    ws.onerror = () => {};
    ws.onclose = (event) => {
      const end = outcome(event.code, errorCode ?? (event.reason || undefined));
      this.lost(end === "retry" ? undefined : end);
    };
  }

  redial(): void {
    if (this.closed) return;
    if (this.ws && this.onlineAt === undefined && Date.now() - this.dialedAt < REDIAL_SETTLE_MS) return;
    this.backoff = BACKOFF.first;
    this.connect();
  }

  send(msg: Outbound): string {
    const id = this.nextId();
    if (this.onlineAt !== undefined) this.ws?.send(JSON.stringify({ ...msg, id }));
    return id;
  }

  close(): void {
    this.closed = true;
    this.drop();
    this.onState("idle");
  }

  private nextId(): string {
    return `c${++this.seq}`;
  }

  private lost(end?: Exclude<Outcome, "retry">): void {
    const lasted = this.onlineAt === undefined ? 0 : Date.now() - this.onlineAt;
    this.drop();
    if (end) {
      this.closed = true;
      this.onState("rejected");
      this.onEnd(end);
      return;
    }
    if (lasted >= BACKOFF.stable) this.backoff = BACKOFF.first;
    this.onState("offline");
    this.retry = setTimeout(() => this.connect(), this.backoff);
    this.backoff = nextBackoff(this.backoff);
  }

  /** Detaches the handlers before closing, so the old socket's close event schedules no retry. */
  private drop(): void {
    clearTimeout(this.retry);
    clearTimeout(this.helloTimer);
    const ws = this.ws;
    this.ws = undefined;
    this.onlineAt = undefined;
    if (!ws) return;
    ws.onopen = null;
    ws.onmessage = null;
    ws.onclose = null;
    ws.close();
  }
}
