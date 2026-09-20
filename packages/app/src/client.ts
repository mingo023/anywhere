import { PROTOCOL_VERSION } from "@pocket/protocol/constants";
import type { ClientMessage, ServerMessage } from "@pocket/protocol";

type Outbound = ClientMessage extends infer M ? (M extends { id: string } ? Omit<M, "id"> : never) : never;
export type ConnectionState = "idle" | "connecting" | "online" | "offline";

const MAX_BACKOFF = 30_000;

export class PocketClient {
  private ws?: WebSocket;
  private seq = 0;
  private backoff = 1000;
  private retry?: ReturnType<typeof setTimeout>;
  private closedByUser = false;

  constructor(
    private readonly url: string,
    private readonly token: string,
    private readonly clientId: string,
    private readonly onMessage: (msg: ServerMessage) => void,
    private readonly onState: (state: ConnectionState) => void,
  ) {}

  connect(): void {
    this.closedByUser = false;
    this.onState("connecting");

    const ws = new WebSocket(this.url);
    this.ws = ws;

    ws.onopen = () => {
      this.backoff = 1000;
      this.send({ type: "hello", token: this.token, clientId: this.clientId, protocolVersion: PROTOCOL_VERSION });
    };

    ws.onmessage = (event) => {
      const msg = JSON.parse(String(event.data)) as ServerMessage;
      if (msg.type === "hello.ok") this.onState("online");
      this.onMessage(msg);
    };

    ws.onerror = () => {};

    ws.onclose = () => {
      this.ws = undefined;
      this.onState("offline");
      if (!this.closedByUser) this.scheduleRetry();
    };
  }

  private scheduleRetry(): void {
    if (this.retry) clearTimeout(this.retry);
    this.retry = setTimeout(() => this.connect(), this.backoff);
    this.backoff = Math.min(this.backoff * 2, MAX_BACKOFF);
  }

  send(msg: Outbound): string {
    const id = `c${++this.seq}`;
    this.ws?.send(JSON.stringify({ ...msg, id }));
    return id;
  }

  close(): void {
    this.closedByUser = true;
    if (this.retry) clearTimeout(this.retry);
    this.ws?.close();
    this.onState("idle");
  }
}
