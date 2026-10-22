// Recreates a design scenario for `pocket-desktop --capture`: git repos under <dir>/home, and a fake
// pocketd (unix socket + phone websocket) under <dir>/pocket. Usage: bun fixture.ts <scenario> <dir>
// The chat scenario's agent starts with the items of chat/$FIXTURE_CHAT.json (default every-tool-kind);
// prompting it "/fixture <name>" plays chat/<name>.json, perf-<items> (perf-1200, perf-5000), the "permission"/"question" ask,
// "exit" (the agent closes), "clear" (a new conversation, as after /clear), "restart" (pocketd restarts mid-turn) or "stream-code" (a reply whose code block streams in over 4.5s).
// "/compact" works for 4s, then compacts. FIXTURE_REFUSE=1 refuses every other prompt with pocketd's error frame;
// FIXTURE_SEND_NOW=1 says Claude's chat takes Send now, which ends the step underway so a steer runs at once;
// FIXTURE_REFUSE_STOP=1 refuses every stop the same way; FIXTURE_STOP_MS delays a stop's interrupted result, as Claude's lags. FIXTURE_CHUNKS sets how many 30ms chunks a reply streams in (default 40).
import { appendFileSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { createConnection, createServer, type Socket } from "node:net";
import { dirname, join } from "node:path";

type Agent = {
  id: string; title: string; provider: "claude" | "codex"; repo: string; branch?: string;
  status: "running" | "idle"; ago: number; waiting?: string; failed?: boolean; model?: string; effort?: string; pinned?: boolean;
  diff?: [string, number, number][]; children?: string[][]; term?: string; restore?: string;
};
type Scenario = { repos: string[]; merged?: string[]; agents: Agent[] };

const RESTORE: [string, number, number][] = [["hooks/use-restore-preview.ts", 28, 11], ["components/terminal-view.tsx", 11, 5], ["hooks/index.ts", 3, 1]];
const OTA: [string, number, number][] = [["src/ota/resume.ts", 60, 36], ["src/ota/store.ts", 24, 15]];
const OTHERS: Agent[] = [
  { id: "ios", title: "Fix keyboard inset", provider: "codex", repo: "app-ios", status: "idle", ago: 4, waiting: "xcodebuild test" },
  { id: "hook", title: "Retry webhook deliveries", provider: "claude", repo: "app-hook", status: "running", ago: 1 },
  { id: "mkt", title: "Draft launch post", provider: "codex", repo: "marketing", status: "running", ago: 3 },
];

const SCENARIOS: Record<string, Scenario> = {
  sessions: {
    repos: ["app-android", "app-ios", "app-hook", "marketing", "cs"],
    agents: [
      { id: "fix", title: "Fix stale terminal reveal", provider: "codex", repo: "app-android", branch: "fix/restore-handoff", status: "idle", ago: 2, waiting: "pnpm test --filter app-android", diff: RESTORE, term: "codex" },
      { id: "split", title: "Split restore hook into two files", provider: "claude", model: "opus", repo: "app-android", branch: "refactor/restore-hook", status: "running", ago: 0, children: [["2", "sub-agents"], ["pnpm", "dev"]] },
      { id: "migrate", title: "Migrate legacy hooks", provider: "codex", repo: "app-android", branch: "chore/migrate-hooks", status: "running", ago: 6 },
      { id: "crash", title: "Crash on resume after OTA", provider: "codex", repo: "app-android", branch: "fix/ota-resume", status: "idle", ago: 22, diff: OTA },
      { id: "rn", title: "Upgrade to RN 0.81", provider: "codex", repo: "app-android", branch: "chore/rn-081", status: "idle", ago: 180, failed: true },
      { id: "haptics", title: "Add haptics to approval sheet", provider: "claude", model: "opus", repo: "app-android", branch: "feat/haptics", status: "idle", ago: 300, diff: OTA },
      ...OTHERS,
    ],
  },
  worktrees: {
    repos: ["app-android", "app-ios", "shared-ui", "app-hook", "marketing", "cs"],
    merged: ["fix/ota-resume"],
    agents: [
      { id: "fix", title: "Fix stale terminal reveal", provider: "codex", repo: "app-android", branch: "fix/restore-handoff", status: "idle", ago: 2, waiting: "pnpm test --filter app-android", diff: RESTORE, term: "codex" },
      { id: "tests", title: "Write tests for restore handoff", provider: "claude", model: "opus", effort: "high", repo: "app-android", branch: "fix/restore-handoff", status: "running", ago: 0, children: [["pnpm", "test", "--watch"]] },
      { id: "callers", title: "Explore restore callers", provider: "codex", repo: "app-android", branch: "fix/restore-handoff", status: "idle", ago: 40 },
      { id: "repro", title: "Reproduce stale flash on emulator", provider: "codex", repo: "app-android", branch: "fix/restore-handoff", status: "idle", ago: 60, pinned: true },
      { id: "split", title: "Split restore hook", provider: "claude", repo: "app-android", branch: "refactor/restore-hook", status: "running", ago: 1 },
      { id: "migrate", title: "Migrate legacy hooks", provider: "codex", repo: "app-android", branch: "chore/migrate-hooks", status: "running", ago: 5 },
      { id: "ota", title: "OTA resume crash", provider: "codex", repo: "app-android", branch: "fix/ota-resume", status: "idle", ago: 90 },
      ...OTHERS,
    ],
  },
};

SCENARIOS.chat = {
  repos: ["app-android", "app-ios", "app-hook", "marketing"],
  agents: [
    { id: "parser", title: "Refactor the transcript fold", provider: "claude", model: "opus", effort: "high", repo: "app-android", branch: "refactor/fold", status: "idle", ago: 1 },
    ...OTHERS,
  ],
};

const RESTORED: Record<string, string> = { fix: "resumed", crash: "interrupted", haptics: "access_lowered", rn: "failed" };
SCENARIOS.restored = { ...SCENARIOS.sessions, agents: SCENARIOS.sessions.agents.map((a) => ({ ...a, restore: RESTORED[a.id] })) };

const [name, arg] = process.argv.slice(2);
const scenario = SCENARIOS[name];
if (!scenario || !arg) throw new Error(`usage: bun fixture.ts <${Object.keys(SCENARIOS).join("|")}> <dir>`);
rmSync(arg, { recursive: true, force: true });
mkdirSync(join(arg, "pocket"), { recursive: true });
// git reports worktree paths resolved (/tmp → /private/tmp); agent cwds must match them.
const dir = realpathSync(arg);
const home = join(dir, "home");
const pocket = join(dir, "pocket");

function git(cwd: string, ...args: string[]) {
  const r = Bun.spawnSync(["git", "-C", cwd, ...args], { env: { ...process.env, HOME: home, GIT_CONFIG_NOSYSTEM: "1" } });
  if (r.exitCode !== 0) throw new Error(`git ${args.join(" ")}: ${r.stderr}`);
}
function write(path: string, text: string) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text);
}
const lines = (tag: string, n: number) => Array.from({ length: n }, (_, i) => `export const ${tag}${i} = ${i};`).join("\n") + "\n";
const repoPath = (r: string) => join(home, "code", r);
const treePath = (r: string, b: string) => join(home, "code", `${r}.worktrees`, b.replace("/", "-"));

for (const r of scenario.repos) {
  const p = repoPath(r);
  mkdirSync(p, { recursive: true });
  git(p, "init", "-q", "-b", "main");
  git(p, "config", "user.name", "Minh Ngo");
  git(p, "config", "user.email", "minh@example.com");
  for (const a of scenario.agents.filter((a) => a.repo === r)) for (const [f, , removed] of a.diff ?? []) write(join(p, f), lines("base", removed + 10));
  write(join(p, "README.md"), `# ${r}\n`);
  git(p, "add", "-A");
  git(p, "commit", "-q", "-m", "Initial commit");
}
const trees = new Map<string, Agent[]>();
for (const a of scenario.agents.filter((a) => a.branch)) trees.set(`${a.repo}\t${a.branch}`, [...(trees.get(`${a.repo}\t${a.branch}`) ?? []), a]);
for (const [key, agents] of trees) {
  const [r, b] = key.split("\t");
  const w = treePath(r, b);
  git(repoPath(r), "worktree", "add", "-q", "-b", b, w, "main");
  if (!scenario.merged?.includes(b)) {
    write(join(w, "NOTES.md"), `${b}\n`);
    git(w, "add", "-A");
    git(w, "commit", "-q", "-m", `Start ${b}`);
  }
  for (const [f, added, removed] of agents.flatMap((a) => a.diff ?? [])) write(join(w, f), lines("next", added) + lines("base", removed + 10).split("\n").slice(removed).join("\n"));
}

type Item = { id: string; kind: string; [k: string]: unknown };
const CHAT_AGENT = "parser";
const perf = (items: number): Item[] =>
  Array.from({ length: items / 8 }, (_, t) => {
    const long = t % 5 === 0 ? Array.from({ length: 12 }, (_, s) => `## Step ${s + 1}\n\nThe fold groups consecutive tool calls so the transcript reads as a conversation.\n\n- one row per block\n- code in \`mono\``).join("\n\n") : "Done: the rows fold as expected.";
    const diff = { lines: [{ number: t + 1, kind: "del", text: `let a${t} = 0;` }, { number: t + 1, kind: "add", text: `let a${t} = 1;` }], additions: 1, deletions: 1 };
    const tool = (n: number, name: string, detail: object) => ({ id: `${t}-t${n}`, kind: "tool", call: { toolUseId: `tu-${t}-${n}`, name, detail, status: "ok", output: "ok", durationMs: 40 * n } });
    return [
      { id: `${t}-u`, kind: "user", text: `Turn ${t + 1}: refactor module ${t}.` },
      { id: `${t}-th`, kind: "thinking", text: `Module ${t} has one caller; the change is local.` },
      { id: `${t}-a1`, kind: "assistant", text: `Reading module ${t} first.` },
      tool(1, "Bash", { kind: "shell", command: `cargo test -p m${t}` }),
      tool(2, "Read", { kind: "read", path: `src/m${t}.rs` }),
      tool(3, "Edit", { kind: "edit", path: `src/m${t}.rs`, diff }),
      { id: `${t}-a2`, kind: "assistant", text: long },
      { id: `${t}-r`, kind: "result", ok: true, durationMs: 30_000, usage: { inputTokens: 9000, outputTokens: 1200 } },
    ];
  }).flat();
const chatItems = (name: string): Item[] =>
  /^perf-\d+$/.test(name) ? perf(Number(name.slice(5))) : JSON.parse(readFileSync(join(import.meta.dir, "chat", `${name}.json`), "utf8"));
const firstChat = chatItems(process.env.FIXTURE_CHAT ?? "every-tool-kind");
const seqs = new Map<string, number>();
// Numbered from the agent's last item, so a played scenario lands after what's shown.
const stamp = (agentId: string, items: Item[], prefix = "") =>
  items.map((it) => {
    const seq = (seqs.get(agentId) ?? 0) + 1;
    seqs.set(agentId, seq);
    // Tool ids too, or a played scenario's calls would update the shown ones that share them.
    const call = it.call as { toolUseId: string } | undefined;
    const tools = prefix && { ...(call && { call: { ...call, toolUseId: prefix + call.toolUseId } }), ...(it.parentToolUseId ? { parentToolUseId: prefix + it.parentToolUseId } : {}) };
    return { ...it, ...tools, id: prefix + it.id, seq, ts: Date.now() - 60_000 + seq * 100 };
  });
// A chat left on a user item streams its reply; one left mid-turn stays working. A compaction after a result leaves it idle.
const lastWork = firstChat.findLast((i) => i.kind !== "compact");
const chatWorking = !!lastWork && lastWork.kind !== "result";
if (chatWorking && scenario.agents[0]?.id === CHAT_AGENT) scenario.agents[0].status = "running";
// FIXTURE_IDLE: nothing running anywhere, so no sidebar spinner redraws the window; for measuring idle cost.
if (process.env.FIXTURE_IDLE) for (const a of scenario.agents) a.status = "idle";

const now = Date.now();
const cwd = (a: Agent) => (a.branch ? treePath(a.repo, a.branch) : repoPath(a.repo));
const status = (a: Agent) => (a.waiting ? "needsYou" : a.failed ? "done" : a.status === "running" ? "working" : "idle");
const left = { claude: 62, codex: 41 };
const summaries = scenario.agents.map((a) => ({
  id: a.id, terminalId: a.id, attached: true, failed: !!a.failed, title: a.title, cwd: cwd(a), provider: a.provider, model: a.model, effort: a.effort, pinned: a.pinned, status: status(a), restore: a.restore,
  createdAt: now - (a.ago + 30) * 60_000, updatedAt: now - a.ago * 60_000,
  tokensUsed: (100 - left[a.provider]) * 2000, contextWindow: 200_000, resets: 0,
}));
const timeline = (a: Agent) => a.id === CHAT_AGENT ? stamp(a.id, firstChat) : [{
  id: `${a.id}-result`, seq: 1, ts: now - a.ago * 60_000, kind: "result", ok: !a.failed, error: a.failed ? "exit 1" : "",
  usage: { inputTokens: (100 - left[a.provider]) * 2000, cacheReadTokens: 0 },
}];

const children = scenario.agents.flatMap((a) => (a.children ?? []).map((argv, i) => ({ id: `${a.id}-tab${i}`, parent: a.id, argv, cwd: cwd(a) })));
const sessions = [
  ...scenario.agents.map((a) => ({ id: a.id, cmd: a.term ?? a.provider, args: [], cwd: cwd(a) })),
  ...children.map((c) => ({ id: c.id, cmd: c.argv[0], args: c.argv.slice(1), cwd: c.cwd })),
];

const SCOPES = process.env.FIXTURE_OBSERVE ? ["observe"] : ["observe", "drive", "approve", "spawn", "owner"];
const port = 45000 + Math.floor(Math.random() * 5000);
write(join(pocket, "config.json"), JSON.stringify({ token: "fixture", port }));
const tracked = [...trees.keys()].map((key) => treePath(...(key.split("\t") as [string, string])));
write(join(pocket, "desktop.json"), JSON.stringify({ projects: scenario.repos.map(repoPath), children: children.map((c) => [c.id, c.parent]), repos: {}, tracked }));

const ESC = "\x1b[";
const fg = (hex: string) => `${ESC}38;2;${parseInt(hex.slice(0, 2), 16)};${parseInt(hex.slice(2, 4), 16)};${parseInt(hex.slice(4, 6), 16)}m`;
const [GREEN, ORANGE, TEAL, DIM, RESET, BOLD, NORMAL] = [fg("18794e"), fg("ad5700"), fg("0e7c86"), fg("8b8b94"), `${ESC}0m`, `${ESC}1m`, `${ESC}22m`];
const HIGHLIGHT = `${ESC}48;2;251;237;212m`;
const box = (inner: string, pad: number) => `${ORANGE}  │ ${RESET}${inner}${ORANGE}${" ".repeat(pad)}│${RESET}`;
const CODEX = [
  `${GREEN}• ${RESET}${BOLD}Edited${NORMAL} hooks/use-restore-preview.ts ${DIM}(+28 −11)${RESET}`,
  `${GREEN}• ${RESET}${BOLD}Edited${NORMAL} components/terminal-view.tsx ${DIM}(+11 −5)${RESET}`,
  "",
  "The preview hook revealed the terminal as soon as the socket connected,",
  `before tmux had redrawn. It now waits for ${TEAL}useTerminalSettled()${RESET}, then snaps`,
  "the placeholder away.",
  "",
  `${ORANGE}• ${RESET}${BOLD}Run${NORMAL} pnpm test --filter app-android`,
  "",
  `${ORANGE}  ╭${"─".repeat(59)}╮${RESET}`,
  box(`${BOLD}Allow command?${NORMAL}`, 44),
  box(`${TEAL}$ pnpm test --filter app-android${RESET}`, 26),
  box("", 58),
  box(`${BOLD}${HIGHLIGHT}› 1. Yes${RESET}`, 50),
  box(`  2. Yes, always allow ${TEAL}pnpm test${RESET}`, 26),
  box("  3. No, tell Codex what to do differently", 16),
  `${ORANGE}  ╰${"─".repeat(59)}╯${RESET}`,
  `${DIM}  ⏎ confirm · esc cancel${RESET}`,
];
const ROWS = 37;
const screen = (id: string) => {
  const body = scenario.agents.find((a) => a.id === id)?.term === "codex" ? CODEX : [];
  return `${ESC}?25l` + [...Array(ROWS - body.length).fill(""), ...body].join("\r\n");
};

const MIN = 60_000;
const AUTOMATIONS = [
  { id: "au1", name: "Morning PR review", prompt: "Review the pull requests opened since yesterday and leave comments on anything risky.", provider: "claude", folder: repoPath("app-android"), schedule: { kind: "days", days: [1, 2, 3, 4, 5], time: "09:00" }, enabled: true, nextRunAt: now + 19 * 60 * MIN },
  { id: "au2", name: "Dependency audit", prompt: "Check for outdated or vulnerable dependencies and open a summary.", provider: "codex", folder: repoPath("app-ios"), schedule: { kind: "days", days: [5], time: "17:00" }, enabled: true, nextRunAt: now + 3 * 24 * 60 * MIN },
  { id: "au3", name: "Flaky test triage", prompt: "Find tests that failed and passed on the same commit this week.", provider: "codex", folder: repoPath("app-hook"), schedule: { kind: "interval", everyMin: 120 }, enabled: true, nextRunAt: now + 97 * MIN },
  { id: "au4", name: "Weekly changelog", prompt: "Draft the changelog from merged pull requests.", provider: "claude", folder: repoPath("marketing"), schedule: { kind: "days", days: [1], time: "10:00" }, enabled: false, nextRunAt: 0 },
];
const run = (id: string, automationId: string, status: string, ago: number, why: string, summary: string, took = 0, agent = "") =>
  ({ id, automationId, status, trigger: why === "Run now" ? "manual" : "schedule", why, summary, startedAt: now - ago * MIN, finishedAt: took ? now - (ago - took) * MIN : 0, agentId: agent, terminalId: agent });
const RUNS = [
  run("r1", "au1", "waiting", 8, "Asked to run tests", "Wants to run pnpm test --filter app-android before commenting.", 0, "fix"),
  run("r2", "au3", "running", 3, "Every 2 hours", "Comparing 14 failures against the last 40 runs.", 0, "hook"),
  run("r3", "au3", "succeeded", 125, "Every 2 hours", "Found 2 flaky tests and filed notes.", 6),
  run("r4", "au2", "skipped", 150, "Mac asleep", ""),
  { ...run("r5", "au1", "succeeded", 26 * 60, "Weekdays 09:00", "Reviewed 3 pull requests; 1 needs a second look.", 12), access: "edits", worktree: repoPath("app-android") + "-wt/review-prs" },
  run("r6", "au3", "failed", 26 * 60 + 120, "Every 2 hours", "The test runner exited with status 1.", 2),
  run("r7", "au2", "succeeded", 50 * 60, "Run now", "No vulnerable dependencies.", 4),
  run("r8", "au4", "cancelled", 52 * 60, "Weekly, Monday 10:00", "Stopped before it finished.", 1),
];

const ops = (c: Socket, first: Buffer) => {
  let buf = "";
  const read = (raw: Buffer) => {
    const lines = (buf + raw.toString()).split("\n");
    buf = lines.pop()!;
    for (const line of lines.filter(Boolean)) {
      const op = JSON.parse(line);
      const reply = (msg: object) => c.write(JSON.stringify(msg) + "\n");
      if (op.op === "list") reply({ ev: "terminals", items: sessions });
      if (op.op === "attach") reply({ ev: "snapshot", id: op.id, cols: 140, rows: ROWS, data: Buffer.from(screen(op.id)).toString("base64") });
    }
  };
  read(first);
  c.on("data", read);
};
// pocketd serves the desktop's websocket and line JSON on one socket, told apart by the first byte.
createServer((c) =>
  c.once("data", (first: Buffer) => {
    if (first[0] !== 0x47) return ops(c, first);
    const ws = createConnection(port, "127.0.0.1");
    ws.write(first);
    c.pipe(ws).pipe(c);
  }),
).listen(join(pocket, "pocketd.sock"));

type Ws = { send(msg: string): unknown };
const streams = new Map<string, ReturnType<typeof setInterval>>();
// Stamped now, as pocketd stamps a live item, so the desktop matches a sent prompt to its echo.
const item = (ws: Ws, agentId: string, it: Item) => ws.send(JSON.stringify({ type: "agent.stream", agentId, epoch: 1, item: { ...stamp(agentId, [it])[0], ts: Date.now() } }));
const update = (ws: Ws, agentId: string, status: string) => {
  const s = summaries.find((s) => s.id === agentId);
  if (s) ws.send(JSON.stringify({ type: "agent.update", agent: { ...s, status, updatedAt: Date.now() } }));
};
// As pocketd's Registry.Remove: one update to closed, then the agent is gone from every list.
const exit = (ws: Ws, agentId: string) => {
  clearInterval(streams.get(agentId));
  streams.delete(agentId);
  update(ws, agentId, "closed");
  const i = summaries.findIndex((s) => s.id === agentId);
  if (i >= 0) summaries.splice(i, 1);
};
// As pocketd's SetConversation on /clear: the timeline starts over, the summary's resets moves, then the new conversation streams.
const clear = (ws: Ws, agentId: string) => {
  clearInterval(streams.get(agentId));
  streams.delete(agentId);
  const s = summaries.find((s) => s.id === agentId);
  if (s) s.resets += 1;
  update(ws, agentId, "idle");
  item(ws, agentId, { id: `clear-u-${Date.now()}`, kind: "user", text: "Start over: why does the build fail on a clean checkout?" });
  item(ws, agentId, { id: `clear-a-${Date.now()}`, kind: "assistant", text: "The build script reads `gen/schema.rs`, which only exists after `cargo xtask codegen` has run once. A clean checkout skips that step, so the include fails." });
  item(ws, agentId, { id: `clear-r-${Date.now()}`, kind: "result", ok: true, durationMs: 2400, usage: { inputTokens: 3000, outputTokens: 90 } });
};
// As Claude's /compact: it works a while with nothing in the timeline, then the compaction lands.
const compact = (ws: Ws, agentId: string) => {
  update(ws, agentId, "working");
  setTimeout(() => {
    item(ws, agentId, { id: `c-${Date.now()}`, kind: "compact", trigger: "manual" });
    update(ws, agentId, "idle");
  }, 4000);
};
// As pocketd restarting mid-turn: the agent works on, but resets starts over at 0 and the timeline holds nothing until it writes again.
const restart = (ws: Ws, agentId: string) => {
  clearInterval(streams.get(agentId));
  streams.delete(agentId);
  const s = summaries.find((s) => s.id === agentId);
  if (s) s.resets = 0;
  update(ws, agentId, "working");
  setTimeout(() => reply(ws, agentId), 3000);
};
// A prompt sent mid-reply, which Claude takes once the reply's step ends, by agent.
const steers = new Map<string, () => void>();
const REPLY = "The transcript jumped because a finished tool's row grew by its output before the list measured it. Rows now keep their height until the next frame measures them, so the list stays pinned to the bottom while text streams in.";
const CODE_REPLY = "The fold in Rust:\n\n```rust\npub fn fold(items: &[Item]) -> Vec<Row> {\n    let mut rows = Vec::with_capacity(items.len());\n    for item in items {\n        match item.kind.as_str() {\n            \"tool\" => push_tool(&mut rows, item),\n            \"text\" => rows.push(Row::Text { id: item.id.clone() }),\n            _ => rows.push(Row::from(item)),\n        }\n    }\n    rows.dedup_by(|a, b| a.id() == b.id());\n    rows\n}\n```\n\nRows keep their ids, so the list keeps its place.";
// `chunks` chunks 30ms apart into one item, as pocketd streams a reply.
const reply = (ws: Ws, agentId: string, text = REPLY, chunks = Number(process.env.FIXTURE_CHUNKS) || 40) => {
  const words = text.split(" ");
  const [id, seq] = [`a-${Date.now()}`, (seqs.get(agentId) ?? 0) + 1];
  seqs.set(agentId, seq);
  update(ws, agentId, "working");
  clearInterval(streams.get(agentId));
  let n = 0;
  const timer = setInterval(() => {
    n += 1;
    const part = words.slice(0, Math.ceil((words.length * n) / chunks)).join(" ");
    ws.send(JSON.stringify({ type: "agent.stream", agentId, epoch: 1, item: { id, seq, ts: Date.now(), kind: "assistant", text: part } }));
    if (n < chunks) return;
    clearInterval(timer);
    streams.delete(agentId);
    const steer = steers.get(agentId);
    steers.delete(agentId);
    if (steer) return steer();
    item(ws, agentId, { id: `r-${Date.now()}`, kind: "result", ok: true, durationMs: 1200, usage: { inputTokens: 12000, outputTokens: 180 } });
    update(ws, agentId, "idle");
  }, 30);
  streams.set(agentId, timer);
};
const QUESTIONS = JSON.parse(readFileSync(join(import.meta.dir, "chat", "every-tool-kind.json"), "utf8")).find((i: Item) => i.id === "t9").call.detail;
const ask = (ws: Ws, agentId: string, kind: "permission" | "question") => {
  const detail = kind === "permission" ? { kind: "shell", command: "pnpm test --filter parser", description: "Run the parser tests" } : QUESTIONS;
  const request = { requestId: `${kind}-${agentId}`, agentId, toolName: kind === "permission" ? "Bash" : "AskUserQuestion", detail, ...(kind === "permission" ? { options: [{ id: "always", label: "Yes, and don't ask again for pnpm test" }], feedback: true } : {}) };
  ws.send(JSON.stringify({ type: "permission.request", request }));
};

Bun.serve({
  port,
  fetch: (req, server) => (server.upgrade(req) ? undefined : new Response("fixture")),
  websocket: {
    message(ws, raw) {
      const f = JSON.parse(raw.toString());
      // What the desktop sent, for checks that a click or key reached pocketd.
      if (process.env.FIXTURE_LOG) appendFileSync(process.env.FIXTURE_LOG, raw + "\n");
      if (f.type === "hello") {
        ws.send(JSON.stringify({ type: "hello.ok", id: f.id, serverId: "fixture", hostname: "fixture", protocolVersion: 3, caps: ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "automations.v1", "locals.v1", "settings.v1", "access.v1"], protocol: { min: 3, max: 3 }, scopes: SCOPES, host: { tailnet: false, keepingAwake: true } }));
        ws.send(JSON.stringify({ type: "agent.list", agents: summaries }));
        ws.send(JSON.stringify({ type: "automations", automations: AUTOMATIONS, runs: RUNS }));
        for (const a of scenario.agents.filter((a) => a.waiting)) {
          ws.send(JSON.stringify({ type: "permission.request", request: { requestId: `ask-${a.id}`, agentId: a.id, toolName: "Bash", detail: { kind: "shell", command: a.waiting } } }));
        }
      }
      if (f.type === "pair.begin") {
        const code = "abcdefghijklmnopqrstuv";
        ws.send(JSON.stringify({ type: "pair.offer", id: f.id, url: `anywhere://pair?v=1&h=100.64.0.1:4517&c=${code}`, code, expiresAt: Date.now() + 300_000 }));
      }
      if (f.type === "agent.timeline") {
        const a = scenario.agents.find((a) => a.id === f.agentId);
        if (a) ws.send(JSON.stringify({ type: "agent.timeline", agentId: a.id, items: timeline(a) }));
        if (a?.id === CHAT_AGENT && firstChat.at(-1)?.kind === "user") reply(ws, a.id);
        if (a?.id === CHAT_AGENT && process.env.FIXTURE_CHAT === "permissions") ask(ws, a.id, "permission");
      }
      if (f.type === "agent.prompt") {
        const name = f.text.match(/^\/fixture (\S+)$/)?.[1];
        if (name === "permission" || name === "question") ask(ws, f.agentId, name);
        else if (name === "exit") exit(ws, f.agentId);
        else if (name === "clear") clear(ws, f.agentId);
        else if (name === "restart") restart(ws, f.agentId);
        else if (name === "stream-code") reply(ws, f.agentId, CODE_REPLY, 150);
        else if (name) ws.send(JSON.stringify({ type: "agent.timeline", agentId: f.agentId, items: stamp(f.agentId, chatItems(name), `${name}-`) }));
        // What pocketd sends when the agent left the foreground before the prompt reached it.
        else if (process.env.FIXTURE_REFUSE) ws.send(JSON.stringify({ type: "error", id: f.id, message: "Agent is no longer in the foreground" }));
        else if (f.text === "/compact") compact(ws, f.agentId);
        // pocketd shows a slash command as typed, from Claude's <command-name> markup; the fixture runs none.
        else if (!process.env.FIXTURE_DROP && f.text.startsWith("/")) item(ws, f.agentId, { id: `u-${Date.now()}`, kind: "user", text: f.text });
        else if (!process.env.FIXTURE_DROP) {
          const answer = () => {
            item(ws, f.agentId, { id: `u-${Date.now()}`, kind: "user", text: f.text });
            reply(ws, f.agentId);
          };
          if (streams.has(f.agentId)) steers.set(f.agentId, answer);
          else answer();
        }
      }
      if (f.type === "agent.commands") {
        const commands = [
          { name: "review", description: "Review a pull request", argumentHint: "<pr>" },
          { name: "release-notes", description: "Draft release notes from the merged pull requests" },
          { name: "goal", description: "Set a goal — keep working until the condition is met" },
        ];
        ws.send(JSON.stringify({ type: "agent.commands", id: f.id, agentId: f.agentId, commands }));
      }
      // A session from the New session page: its terminal and agent appear, then the agent answers the prompt.
      if (f.type === "agent.create") {
        const { project, checkout, provider, prompt } = f.spec;
        const id = `new-${f.requestId}`;
        const tree = checkout?.new?.name ? join(`${project}.worktrees`, checkout.new.name) : project;
        mkdirSync(tree, { recursive: true });
        sessions.push({ id, cmd: provider, args: [], cwd: tree });
        summaries.push({ ...summaries[0], id, terminalId: id, title: prompt ?? "New session", cwd: tree, provider, status: "working", pinned: false, createdAt: Date.now(), updatedAt: Date.now() });
        ws.send(JSON.stringify({ type: "agent.creating", id: f.id, requestId: f.requestId, terminalId: id, cwd: tree, setup: false }));
        ws.send(JSON.stringify({ type: "agent.list", agents: summaries }));
        ws.send(JSON.stringify({ type: "agent.created", id: f.id, requestId: f.requestId, agentId: id }));
        if (prompt) {
          item(ws, id, { id: `u-${Date.now()}`, kind: "user", text: prompt });
          reply(ws, id);
        }
      }
      // What pocketd sends when the agent is still resuming and can't take Esc yet.
      if (f.type === "agent.interrupt" && process.env.FIXTURE_REFUSE_STOP) ws.send(JSON.stringify({ type: "error", id: f.id, code: "agent_resuming", message: "This session is still resuming" }));
      else if (f.type === "agent.interrupt") {
        setTimeout(() => {
          clearInterval(streams.get(f.agentId));
          streams.delete(f.agentId);
          item(ws, f.agentId, { id: `r-${Date.now()}`, kind: "result", ok: false, durationMs: 0, error: "Interrupted" });
          update(ws, f.agentId, "idle");
        }, Number(process.env.FIXTURE_STOP_MS) || 0);
      }
      if (f.type === "agent.providers" && process.env.FIXTURE_SEND_NOW) {
        const chat = { interrupt: true, compact: true, sendWhileWorking: true, subAgents: true, sendNow: true };
        ws.send(JSON.stringify({ type: "agent.providers", id: f.id, providers: [{ id: "claude", available: true, efforts: ["low", "medium", "high", "xhigh", "max"], plan: true, chat }], maxAccess: "full", phoneMaxAccess: "ask" }));
      }
      if (f.type === "agent.send_now") {
        const steer = steers.get(f.agentId);
        steers.delete(f.agentId);
        if (steer) {
          clearInterval(streams.get(f.agentId));
          streams.delete(f.agentId);
          steer();
        }
      }
      if (f.type === "permission.resolve") ws.send(JSON.stringify({ type: "permission.resolved", requestId: f.requestId, decision: f.decision }));
    },
  },
});
console.log(`ready ${name}`);
