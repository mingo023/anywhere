// Recreates a design scenario for `pocket-desktop --capture`: git repos under <dir>/home, and a fake
// pocketd (unix socket + phone websocket) under <dir>/pocket. Usage: bun fixture.ts <scenario> <dir>
import { mkdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
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

const now = Date.now();
const cwd = (a: Agent) => (a.branch ? treePath(a.repo, a.branch) : repoPath(a.repo));
const status = (a: Agent) => (a.waiting ? "needsYou" : a.failed ? "done" : a.status === "running" ? "working" : "idle");
const left = { claude: 62, codex: 41 };
const summaries = scenario.agents.map((a) => ({
  id: a.id, terminalId: a.id, attached: true, failed: !!a.failed, title: a.title, cwd: cwd(a), provider: a.provider, model: a.model, effort: a.effort, pinned: a.pinned, status: status(a), restore: a.restore,
  createdAt: now - (a.ago + 30) * 60_000, updatedAt: now - a.ago * 60_000,
  tokensUsed: (100 - left[a.provider]) * 2000, contextWindow: 200_000,
}));
const timeline = (a: Agent) => [{
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

Bun.serve({
  port,
  fetch: (req, server) => (server.upgrade(req) ? undefined : new Response("fixture")),
  websocket: {
    message(ws, raw) {
      const f = JSON.parse(raw.toString());
      if (f.type === "hello") {
        ws.send(JSON.stringify({ type: "hello.ok", id: f.id, serverId: "fixture", hostname: "fixture", protocolVersion: 3, caps: ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "automations.v1", "locals.v1"], protocol: { min: 3, max: 3 }, scopes: SCOPES, host: { tailnet: false, keepingAwake: true } }));
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
      }
    },
  },
});
console.log(`ready ${name}`);
