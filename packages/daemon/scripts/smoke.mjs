import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import WebSocket from "ws";

const DECISION = process.env.POCKET_DECISION ?? "allow";

const config = JSON.parse(readFileSync(join(homedir(), ".coding-pocket", "config.json"), "utf8"));
const cwd = process.argv[2] ?? process.cwd();
const prompt = process.argv[3] ?? "Run `echo hello from pocket` and then tell me what it printed.";

const ws = new WebSocket(`ws://127.0.0.1:${config.port}`);
let n = 0;
const send = (msg) => ws.send(JSON.stringify({ id: `r${++n}`, ...msg }));

ws.on("open", () => send({ type: "hello", token: config.token, clientId: "smoke", protocolVersion: 1 }));

ws.on("message", (raw) => {
  const msg = JSON.parse(String(raw));
  switch (msg.type) {
    case "hello.ok":
      console.log(`[connected] ${msg.hostname}`);
      send({ type: "agent.create", cwd, profileId: "claude-default", prompt, permissionMode: "default" });
      break;
    case "agent.stream": {
      const i = msg.item;
      if (i.kind === "tool") console.log(`  [tool ${i.call.status}] ${i.call.name} ${JSON.stringify(i.call.detail).slice(0, 90)}`);
      else if (i.kind === "result") { console.log(`  [result] ok=${i.ok} ${i.durationMs}ms cost=${i.costUsd ?? "-"}`); ws.close(); }
      else console.log(`  [${i.kind}] ${String(i.text).slice(0, 120).replace(/\n/g, " ")}`);
      break;
    }
    case "permission.request":
      console.log(`  [PERMISSION] ${msg.request.toolName} -> ${DECISION}`);
      send({ type: "permission.resolve", requestId: msg.request.requestId, decision: DECISION });
      break;
    case "agent.update":
      console.log(`[agent ${msg.agent.status}] ${msg.agent.title}`);
      break;
    case "error":
      console.error(`[error] ${msg.message}`);
      ws.close();
      break;
  }
});
ws.on("close", () => process.exit(0));
