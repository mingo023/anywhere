// Recreates a design scenario for `pocket-desktop --capture`: git repos under <dir>/home, and a fake
// pocketd (unix socket + phone websocket) under <dir>/pocket. Usage: bun fixture.ts <scenario> <dir>
// The chat scenario's agent starts with the items of chat/$FIXTURE_CHAT.json (default every-tool-kind);
// prompting it "/fixture <name>" plays chat/<name>.json, perf-<items> (perf-1200, perf-5000), the "permission"/"question" ask,
// "exit" (the agent closes), "clear" (a new conversation, as after /clear), "restart" (pocketd restarts mid-turn), "stream-code" (a reply whose code block streams in over 4.5s)
// or "delegate" (the agent hands three sessions to others: they appear under it, one asks "Which test runner?", one finishes, one fails, and pocketd tells the chat of each).
// FIXTURE_DELEGATE_WAKE=1: each delivery wakes the agent into a short reply, as pocketd does, and the session that asked finishes last, waking it to sum all three up.
// A prompt sent with the queue mode while the agent works waits in pocketd's queue (queue.list, queue.update, edit, cancel, reorder, send_now) and runs once the turn ends; a Stop holds it.
// FIXTURE_NO_ORCHESTRATOR=1 leaves out the orchestrator capability, as an older pocketd does: the desktop then holds sends to a busy agent itself.
// pocketd links two SSH hosts: devbox (online, 1.4.2) and 10.0.4.12 (pocketd stopped there). Reconnecting 10.0.4.12 finds it refusing the
// user's keys; it takes the password hunter2; an install there stops at Verify. An added host comes online after 400ms.
// FIXTURE_HOST_ADD=connecting|failed leaves an added host linking for good, or refuses its keys, instead of coming online.
// FIXTURE_NO_HOSTS=1 leaves out hosts.v1, as a pocketd older than the hosts move, with devbox saved in desktop.json.
// FIXTURE_HOST_FILES=1 runs devbox's own pocketd on its socket, answering status, the file reads the host finder makes and git in its
// repositories; devbox's app-ios is a project, the twin of this Mac's, and 10.0.4.12 (retrying) holds one called api.
// With FIXTURE_HOST_FILES, devbox's git holds a clone unanswered, as one over a slow link would; FIXTURE_CLONE_DONE=1 lets it succeed.
// FIXTURE_HOST_DELAY_MS delays each of devbox's replies, as a host over a real link answers.
// FIXTURE_HOST_GH=1 (with FIXTURE_HOST_FILES) gives devbox's app-ios a worktree, app-ios-login on fix/login, whose PR #42 only devbox's
// `gh` account `work` can see, and app-ios has `work` chosen; its active account, `me`, can't see acme/app-ios.
// FIXTURE_HOST_GH_LIMIT=n (with FIXTURE_HOST_GH) answers devbox's first n `gh pr view` runs, then refuses each as a spent GraphQL rate limit.
// FIXTURE_EXTRA_HOSTS=n adds n more hosts, box-1…box-n, all retrying.
// FIXTURE_HOST_STATUS_FAILS=n refuses devbox's first n status asks, as a pocketd still starting does.
// FIXTURE_FIRST_LAUNCH=<unix seconds> saves that first launch in desktop.json; three days back makes the sidebar ask for a star.
// FIXTURE_BACKGROUND=<image path> saves it as the background image in desktop.json.
// "/compact" works for 4s, then compacts. FIXTURE_REFUSE=1 refuses every other prompt with pocketd's error frame;
// FIXTURE_SEND_NOW=1 says Claude's chat takes Send now, which ends the step underway so a steer runs at once;
// FIXTURE_ECHO_MS delays a new session's prompt reaching its transcript, as Claude takes seconds to start;
// FIXTURE_REFUSE_QUEUE=1 refuses every queue edit, cancel, reorder and send-now as pocketd does once the entry went out.
// FIXTURE_REFUSE_STOP=1 refuses every stop the same way; FIXTURE_STOP_MS delays a stop's interrupted result, as Claude's lags. FIXTURE_CHUNKS sets how many 30ms chunks a reply streams in (default 40).
// The "agents" and "agents-chat" scenarios' pocketd says all ten agents are installed; any other names none, as one older than the eight new ones.
import { appendFileSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { createConnection, createServer, type Socket } from "node:net";
import { dirname, join } from "node:path";

// In the order every list shows them, as agents::providers::ALL holds them.
const PROVIDER_IDS = ["claude", "codex", "cursor", "grok", "opencode", "pi", "omp", "fx", "hermes", "antigravity"] as const;
type Provider = (typeof PROVIDER_IDS)[number];
type Agent = {
  id: string; title: string; provider: Provider; repo: string; branch?: string;
  status: "running" | "idle"; ago: number; waiting?: string; failed?: boolean; model?: string; effort?: string; pinned?: boolean; activity?: string;
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

// A session of each agent pocketd runs beside Claude Code and Codex.
SCENARIOS.agents = {
  repos: SCENARIOS.sessions.repos,
  agents: [
    { id: "fix", title: "Fix stale terminal reveal", provider: "opencode", model: "anthropic/claude-sonnet-4-6", repo: "app-android", branch: "fix/restore-handoff", status: "idle", ago: 2, waiting: "pnpm test --filter app-android", diff: RESTORE },
    { id: "split", title: "Split restore hook into two files", provider: "pi", model: "openai-codex/gpt-5.4", effort: "high", repo: "app-android", branch: "refactor/restore-hook", status: "running", ago: 0 },
    { id: "migrate", title: "Migrate legacy hooks", provider: "hermes", model: "openai-codex:gpt-6-sol", repo: "app-android", branch: "chore/migrate-hooks", status: "running", ago: 6 },
    { id: "crash", title: "Crash on resume after OTA", provider: "cursor", model: "composer-2.5", repo: "app-android", branch: "fix/ota-resume", status: "idle", ago: 22, diff: OTA },
    { id: "rn", title: "Upgrade to RN 0.81", provider: "grok", model: "grok-4.6", repo: "app-android", branch: "chore/rn-081", status: "idle", ago: 180, failed: true },
    { id: "haptics", title: "Add haptics to approval sheet", provider: "antigravity", model: "gemini-3.8-flash-high", repo: "app-android", branch: "feat/haptics", status: "idle", ago: 300, diff: OTA },
    { id: "lint", title: "Fix the lint warnings", provider: "omp", repo: "app-android", status: "idle", ago: 320 },
    { id: "docs", title: "Document the restore flow", provider: "fx", model: "zai/glm-5.2-fast", repo: "app-android", status: "idle", ago: 340 },
    ...OTHERS,
  ],
};
// The chat is Pi's, which never asks before it acts.
SCENARIOS["agents-chat"] = {
  repos: SCENARIOS.chat.repos,
  agents: [{ ...SCENARIOS.chat.agents[0], provider: "pi", model: "anthropic/claude-sonnet-4-6", effort: "high" }, ...OTHERS],
};

const RESTORED: Record<string, string> = { fix: "resumed", crash: "interrupted", haptics: "access_lowered", rn: "failed" };
// Activity's card and drawer: the sessions, with what the prototype's sample data says of them (its working line, and a diff its Read and Edited counts and per-file split come out to).
const ACTIVITY_DIFF: [string, number, number][] = [["components/terminal-view.tsx", 25, 9], ["hooks/index.ts", 17, 6], ["hooks/use-restore-preview.ts", 8, 3]];
const DOING: Record<string, string> = { split: "Editing src/hooks/use-restore.ts", migrate: "Reading src/hooks/legacy.ts", hook: "Running pnpm test --filter webhooks", mkt: "Drafting the launch post" };
SCENARIOS.activity = { ...SCENARIOS.sessions, agents: SCENARIOS.sessions.agents.map((a) => ({ ...a, ...(a.id === "fix" && { diff: ACTIVITY_DIFF }), ...(DOING[a.id] && { activity: DOING[a.id] }) })) };
SCENARIOS.restored = { ...SCENARIOS.sessions, agents: SCENARIOS.sessions.agents.map((a) => ({ ...a, restore: RESTORED[a.id] })) };

const [name, arg] = process.argv.slice(2);
const scenario = SCENARIOS[name];
if (!scenario || !arg) throw new Error(`usage: bun fixture.ts <${Object.keys(SCENARIOS).join("|")}> <dir>`);
// FIXTURE_SESSIONS=n adds n sessions, each on its own worktree, spread over the scenario's repos: a long day's sidebar.
for (let i = 0; i < Number(process.env.FIXTURE_SESSIONS ?? 0); i++)
  scenario.agents.push({ id: `load${i}`, title: `Load session ${i}`, provider: i % 2 ? "claude" : "codex", repo: scenario.repos[i % scenario.repos.length], branch: `load/s${i}`, status: i % 3 ? "idle" : "running", ago: i });
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
// FIXTURE_CHANGED=n changes n more files across 40 folders in the first repo's first worktree, the size of a big refactor.
const CHANGED = Number(process.env.FIXTURE_CHANGED ?? 0);
const changedFile = (i: number) => `src/gen/m${i % 40}/f${i}.ts`;
const lines = (tag: string, n: number) => Array.from({ length: n }, (_, i) => `export const ${tag}${i} = ${i};`).join("\n") + "\n";
const repoPath = (r: string) => join(home, "code", r);
const treePath = (r: string, b: string) => join(home, "code", `${r}.worktrees`, b.replace("/", "-"));

for (const r of scenario.repos) {
  const p = repoPath(r);
  mkdirSync(p, { recursive: true });
  git(p, "init", "-q", "-b", "main");
  git(p, "config", "user.name", "Dev User");
  git(p, "config", "user.email", "dev@example.com");
  for (const a of scenario.agents.filter((a) => a.repo === r)) for (const [f, , removed] of a.diff ?? []) write(join(p, f), lines("base", removed + 10));
  write(join(p, "README.md"), `# ${r}\n`);
  if (CHANGED && r === scenario.repos[0]) for (let i = 0; i < CHANGED; i++) write(join(p, changedFile(i)), lines("base", 6));
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
  if (CHANGED && r === scenario.repos[0] && trees.keys().next().value === key) for (let i = 0; i < CHANGED; i++) write(join(w, changedFile(i)), lines("next", 2) + lines("base", 6));
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
const left: Record<Provider, number> = { claude: 62, codex: 41, cursor: 77, grok: 58, opencode: 49, pi: 83, omp: 91, fx: 66, hermes: 35, antigravity: 72 };
const summaries = scenario.agents.map((a) => ({
  id: a.id, terminalId: a.id, attached: true, failed: !!a.failed, title: a.title, cwd: cwd(a), provider: a.provider, model: a.model, effort: a.effort, pinned: a.pinned, chatOnly: !!process.env.FIXTURE_HOST_ONLY, status: status(a), restore: a.restore, activity: a.activity ?? "",
  createdAt: now - (a.ago + 30) * 60_000, updatedAt: now - a.ago * 60_000,
  tokensUsed: (100 - left[a.provider]) * 2000, contextWindow: 200_000, resets: 0,
}));
// A session waiting on a permission is mid-turn: its prompt, what it read and edited, and why it asks.
const asking = (a: Agent): Item[] => {
  const tool = (kind: string, path: string, i: number) => ({ id: `${a.id}-${kind}-${i}`, kind: "tool", call: { toolUseId: `${a.id}-${kind}-${i}`, name: kind, status: "ok", detail: { kind, path } } });
  const files = (a.diff ?? []).map(([path]) => path);
  return [
    { id: `${a.id}-user`, kind: "user", text: a.title },
    ...["package.json", ...files].map((path, i) => tool("read", path, i)),
    ...files.map((path, i) => tool("edit", path, i)),
    { id: `${a.id}-said`, kind: "assistant", text: `Wants to run the ${a.repo} tests.` },
  ] as Item[];
};
// Only Activity's screens show a turn's prompt and steps; the others keep the one result they were reviewed with.
const turns = name === "activity";
const timeline = (a: Agent) => a.id === CHAT_AGENT ? stamp(a.id, firstChat) : turns && a.waiting ? stamp(a.id, asking(a)) : [...(turns && a.failed ? [{ id: `${a.id}-user`, seq: 0, ts: now - (a.ago + 1) * 60_000, kind: "user", text: a.title }] : []), {
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
const legacyHosts = process.env.FIXTURE_NO_HOSTS ? [{ id: "devbox", name: "devbox", target: { kind: "alias", name: "devbox" } }] : [];
const hostProjects = process.env.FIXTURE_HOST_FILES ? ["//devbox/home/dev/code/app-ios", "//10-0-4-12/home/dev/api"] : [];
if (process.env.FIXTURE_HOST_GH) tracked.push("//devbox/home/dev/code/app-ios-login");
write(join(pocket, "desktop.json"), JSON.stringify({ projects: [...scenario.repos.map(repoPath), ...hostProjects], children: children.map((c) => [c.id, c.parent]), repos: process.env.FIXTURE_HOST_GH ? { "//devbox/home/dev/code/app-ios": { gh_account: "work" } } : {}, tracked, hosts: legacyHosts, first_launch: process.env.FIXTURE_FIRST_LAUNCH ? Number(process.env.FIXTURE_FIRST_LAUNCH) : undefined, appearance: process.env.FIXTURE_BACKGROUND ? { background: process.env.FIXTURE_BACKGROUND } : undefined }));

type HostTarget = { kind: "alias"; name: string } | { kind: "address"; addr: string; port: number; user?: string; keyFile?: string };
type Host = { id: string; name: string; target: HostTarget; socket: string; link: { state: string; version?: string; uptime?: number; failure?: string } };
const hostEntry = (id: string, name: string, target: HostTarget, link: Host["link"]): Host => ({ id, name, target, socket: join(pocket, "hosts", `${id}.sock`), link });
const ONLINE = { state: "online", version: "1.4.2", uptime: 3 * 86_400 };
const hosts: Host[] = [
  hostEntry("devbox", "devbox", { kind: "alias", name: "devbox" }, ONLINE),
  hostEntry("10-0-4-12", "10.0.4.12", { kind: "address", addr: "10.0.4.12", port: 2222, user: "dev" }, { state: "retrying", failure: "not_running" }),
  ...Array.from({ length: Number(process.env.FIXTURE_EXTRA_HOSTS ?? 0) }, (_, i) => hostEntry(`box-${i + 1}`, `box-${i + 1}`, { kind: "alias", name: `box-${i + 1}` }, { state: "retrying", failure: "not_running" })),
];
const pushHost = (ws: Ws, h: Host, id?: string) => ws.send(JSON.stringify({ type: "host.update", ...(id && { id }), host: h }));
const setLink = (ws: Ws, hostId: string, link: Host["link"]) => {
  const h = hosts.find((h) => h.id === hostId);
  if (h) pushHost(ws, Object.assign(h, { link }));
};
const hostVerb = (ws: Ws, f: { type: string; id: string; hostId: string; target: HostTarget; password: string; update: boolean }) => {
  const ack = () => ws.send(JSON.stringify({ type: "ack", id: f.id }));
  if (f.type === "host.list") ws.send(JSON.stringify({ type: "host.list", id: f.id, hosts }));
  if (f.type === "host.add") {
    const t = f.target;
    const taken = hosts.find((h) => JSON.stringify(h.target) === JSON.stringify(t));
    if (taken) return ws.send(JSON.stringify({ type: "error", id: f.id, code: "host_exists", message: `${taken.name} is already added` }));
    const name = t.kind === "alias" ? t.name : /^[\d.]+$/.test(t.addr) ? t.addr : t.addr.split(".")[0];
    const h = hostEntry(name.toLowerCase().replace(/[^a-z0-9]+/g, "-"), name, t, { state: "connecting" });
    hosts.push(h);
    pushHost(ws, h, f.id);
    const settle = process.env.FIXTURE_HOST_ADD;
    if (settle !== "connecting") setTimeout(() => setLink(ws, h.id, settle === "failed" ? { state: "failed", failure: "auth" } : ONLINE), 400);
  }
  if (f.type === "host.retry") {
    ack();
    if (f.hostId === "10-0-4-12") setLink(ws, f.hostId, { state: "failed", failure: "auth" });
  }
  if (f.type === "host.sign_in" && f.password === "hunter2") {
    ack();
    setLink(ws, f.hostId, ONLINE);
  } else if (f.type === "host.sign_in") ws.send(JSON.stringify({ type: "error", id: f.id, code: "host_ssh", detail: "auth", message: "Wrong password." }));
  if (f.type === "host.install") {
    ack();
    for (const progress of [{ kind: "step", step: "detect" }, { kind: "step", step: "verify" }]) ws.send(JSON.stringify({ type: "host.progress", hostId: f.hostId, update: !!f.update, progress }));
  }
  if (f.type === "host.remove") {
    const at = hosts.findIndex((h) => h.id === f.hostId);
    if (at < 0) return ws.send(JSON.stringify({ type: "error", id: f.id, code: "unknown_host", message: "That host isn't added" }));
    ack();
    hosts.splice(at, 1);
    ws.send(JSON.stringify({ type: "host.list", hosts }));
  }
};

// What pocketd says of each agent after Claude Code: its efforts, whether it plans, and what its chat driver does.
const THINKING = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
const EFFORTS: Partial<Record<Provider, string[]>> = { grok: ["low", "medium", "high", "xhigh"], pi: THINKING, omp: THINKING };
const provider = (id: Provider) => ({
  id, available: true, efforts: EFFORTS[id] ?? [], plan: ["opencode", "grok", "pi", "omp"].includes(id), neverAsks: ["pi", "omp"].includes(id) || undefined,
  chat: { interrupt: true, compact: ["codex", "pi", "omp"].includes(id), sendWhileWorking: id === "codex", subAgents: false, sendNow: false },
});

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
  // Made by a session's agent; the session "gone" is not in any scenario, so it reads as closed.
  { id: "au5", name: "Re-run tests on push", prompt: "Run the test suite and report what broke.", provider: "codex", folder: repoPath("app-android"), schedule: { kind: "webhook" }, enabled: true, createdBy: "fix", bound: "fix", webhook: true, webhookUrl: "http://127.0.0.1:47213/hooks/au5" },
  { id: "au6", name: "Nightly lint sweep", prompt: "Lint the repo and fix what is safe.", provider: "claude", folder: repoPath("app-ios"), schedule: { kind: "days", days: [1, 2, 3, 4, 5], time: "23:00" }, enabled: true, nextRunAt: now + 9 * 60 * MIN, createdBy: "gone", bound: "gone" },
];
const ticket = (id: string, title: string, state: string, status: string, ago: number, extra: Record<string, unknown> = {}) =>
  ({ id, number: +id.slice(1), title, prompt: `${title}.`, provider: "claude", folder: repoPath("app-android"), state, order: ago, status, createdAt: now - ago * 2 * MIN, changedAt: now - ago * MIN, ...extra });
const TICKETS = [
  ticket("t1", "Virtualize the Explorer tree", "todo", "", 40, { priority: 2, labels: ["Feature"] }),
  ticket("t2", "Retry the pocketd link with backoff", "todo", "", 41, { provider: "codex", priority: 3, labels: ["Infra"] }),
  ticket("t3", "Fix the scroll jump on resize in chat", "queued", "", 30, { priority: 1, labels: ["Bug"] }),
  ticket("t4", "Version the settings store", "queued", "", 31, { provider: "codex" }),
  ticket("t5", "Fold unchanged hunks in the diff", "live", "working", 3, { agentId: "split", terminalId: "split", worktree: "/tmp/wt/fold-hunks", priority: 2 }),
  ticket("t6", "Refresh the phone pairing QR", "live", "starting", 1, { provider: "codex", worktree: "/tmp/wt/pairing-qr" }),
  ticket("t7", "Trim scrollback under memory pressure", "live", "needsYou", 7, { agentId: "fix", terminalId: "fix", worktree: "/tmp/wt/trim-scrollback", priority: 1, labels: ["Bug"] }),
  ticket("t8", "Paste images from the clipboard", "live", "failed", 10, { provider: "codex", why: "cargo test failed in composer.rs", worktree: "/tmp/wt/paste-images", labels: ["Bug"] }),
  ticket("t9", "Rename groups inline in the sidebar", "live", "done", 4, { agentId: "crash", terminalId: "crash", worktree: "/tmp/wt/rename-groups" }),
  ticket("t10", "Keyboard navigation in Inbox", "live", "idle", 25, { agentId: "haptics", terminalId: "haptics", worktree: "/tmp/wt/inbox-keys" }),
  ticket("t11", "Reveal worktree in Finder", "closed", "idle", 15, { closedWhy: "merged", closedAt: now - 15 * MIN, worktree: "/tmp/wt/reveal" }),
  ticket("t12", "Try a denser session list", "closed", "idle", 150, { closedWhy: "closed", closedAt: now - 150 * MIN, worktree: "/tmp/wt/denser" }),
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

// Two Claude Code logins, so a chat shows its account chip; the built-in one is unlabelled, as most are.
const usage = (session: number, week: number) => ({ session: { percent: session, resetsAt: now + 120 * MIN }, week: { percent: week, resetsAt: now + 4 * 1440 * MIN }, at: now });
const ACCOUNTS = [
  { id: "builtin", provider: "claude", email: "me@example.com", plan: "Max", signedIn: true, default: true, usage: usage(42, 72) },
  { id: "work", provider: "claude", label: "Work", email: "work@example.com", plan: "Team", signedIn: true, usage: usage(100, 64) },
];

// The sockets of the desktop's line-JSON channel, to tell of terminals that appear while it runs.
const conns = new Set<Socket>();
const ops = (c: Socket, first: Buffer) => {
  conns.add(c);
  c.on("close", () => conns.delete(c));
  let buf = "";
  const read = (raw: Buffer) => {
    const lines = (buf + raw.toString()).split("\n");
    buf = lines.pop()!;
    for (const line of lines.filter(Boolean)) {
      const op = JSON.parse(line);
      const reply = (msg: object) => c.write(JSON.stringify(msg) + "\n");
      if (op.op === "list") reply({ ev: "terminals", items: sessions });
      if (op.op === "accounts" || op.op === "accounts.usage") reply({ ev: "accounts", accounts: ACCOUNTS, autoSwitch: true });
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

// devbox's folders, by path: a folder holds its names (a trailing / marks one), a file its text.
const HOST_FILES: Record<string, string[] | string> = {
  "/": ["etc/", "home/", "opt/"],
  "/etc": ["hostname"],
  "/etc/hostname": "devbox\n",
  "/opt": [],
  "/home": ["dev/"],
  "/home/dev": [".bashrc", ".ssh/", "code/", "Downloads/", "Library/", "notes.md"],
  "/home/dev/Library": Array.from({ length: 2000 }, (_, i) => `item-${String(i).padStart(4, "0")}${i % 3 ? "/" : ""}`),
  "/home/dev/.bashrc": 'export PATH="$HOME/.cargo/bin:$PATH"\nalias gs="git status"\n',
  "/home/dev/.ssh": ["config"],
  "/home/dev/.ssh/config": "Host github.com\n  IdentityFile ~/.ssh/id_ed25519\n",
  "/home/dev/Downloads": [],
  "/home/dev/notes.md": "# Notes\n\n- set up the build box\n- clone app-android here\n",
  "/home/dev/code": ["api/", "app/", "app-ios/", "scratch/", "todo.txt"],
  "/home/dev/code/app-ios": [".git/", "Sources/", "Package.swift"],
  "/home/dev/code/app-ios/.git": ["HEAD", "config"],
  "/home/dev/code/app-ios/.git/HEAD": "ref: refs/heads/main\n",
  "/home/dev/code/app-ios/.git/config": '[remote "origin"]\n\turl = git@github.com:acme/app-ios.git\n',
  "/home/dev/code/app-ios/Sources": [],
  "/home/dev/code/app-ios/Package.swift": "// swift-tools-version:5.9\n",
  "/home/dev/code/todo.txt": "try the relay idea\n",
  "/home/dev/code/scratch": ["try.sh"],
  "/home/dev/code/scratch/try.sh": "#!/bin/sh\necho hi\n",
  "/home/dev/code/api": [".git/", "cmd/", "go.mod"],
  "/home/dev/code/api/.git": ["HEAD", "config"],
  "/home/dev/code/api/.git/HEAD": "ref: refs/heads/develop\n",
  "/home/dev/code/api/.git/config": '[remote "origin"]\n\turl = git@github.com:acme/api.git\n',
  "/home/dev/code/api/cmd": [],
  "/home/dev/code/api/go.mod": "module github.com/acme/api\n\ngo 1.23\n",
  "/home/dev/code/app": [".git/", "node_modules/", "src/", "package.json", "README.md"],
  "/home/dev/code/app/.git": ["HEAD", "config"],
  "/home/dev/code/app/.git/HEAD": "ref: refs/heads/main\n",
  "/home/dev/code/app/.git/config": '[core]\n\tbare = false\n[remote "origin"]\n\turl = https://github.com/acme/app.git\n',
  "/home/dev/code/app/node_modules": [],
  "/home/dev/code/app/src": ["main.ts"],
  "/home/dev/code/app/src/main.ts": 'import { start } from "./server";\n\nstart({ port: 8080 });\n',
  "/home/dev/code/app/package.json": '{\n  "name": "app",\n  "private": true,\n  "scripts": { "dev": "vite" }\n}\n',
  "/home/dev/code/app/README.md": "# app\n\nThe web app.\n\n## Develop\n\n\tpnpm install\n\tpnpm dev\n",
};
const HOST_LOGIN = "/home/dev/code/app-ios-login";
if (process.env.FIXTURE_HOST_GH) Object.assign(HOST_FILES, { [HOST_LOGIN]: [".git", "Sources/"], [`${HOST_LOGIN}/.git`]: [], [`${HOST_LOGIN}/Sources`]: [] });
const hostEntryOf = (dir: string, name: string) => {
  const folder = name.endsWith("/");
  const path = `${dir === "/" ? "" : dir}/${name.replace(/\/$/, "")}`;
  const body = HOST_FILES[path];
  return { name: name.replace(/\/$/, ""), dir: folder, size: typeof body === "string" ? body.length : 0, mtime: now - 3 * 3_600_000, ignored: name === "node_modules/" };
};
// What git prints in a devbox repository, for the commands the new-session page and the sidebar run.
const hostGit = ([, , dir, ...args]: string[]) => {
  if (!Array.isArray(HOST_FILES[`${dir}/.git`])) return null;
  const run = args.join(" ");
  const at = Math.floor(now / 1000);
  const login = process.env.FIXTURE_HOST_GH && dir.startsWith("/home/dev/code/app-ios") ? `worktree ${HOST_LOGIN}\nHEAD ${"1".repeat(40)}\nbranch refs/heads/fix/login\n\n` : "";
  if (run.startsWith("worktree list")) return `worktree ${dir === HOST_LOGIN ? "/home/dev/code/app-ios" : dir}\nHEAD ${"0".repeat(40)}\nbranch refs/heads/main\n\n${login}`;
  if (run === "rev-parse --abbrev-ref HEAD") return dir === HOST_LOGIN ? "fix/login\n" : "main\n";
  if (run.startsWith("for-each-ref") && run.endsWith(" refs/heads")) return run.includes("committerdate:unix") ? `${at} main\n${at - 86_400} develop\n` : "main\ndevelop\n";
  return "";
};
let prViews = 0;
const hostGhAs = (login: string, cwd: string, args: string[]) => {
  const hidden = { code: 1, stderr: "GraphQL: Could not resolve to a Repository with the name 'acme/app-ios'. (repository)\n" };
  const run = args.join(" ");
  if (run === "--version") return { stdout: "gh version 2.81.0 (2026-01-01)\n" };
  if (run.startsWith("repo view")) return login === "work" ? { stdout: '{"viewerPermission":"WRITE"}' } : hidden;
  if (run.startsWith("pr view") && cwd === HOST_LOGIN && process.env.FIXTURE_HOST_GH_LIMIT && prViews++ >= Number(process.env.FIXTURE_HOST_GH_LIMIT)) {
    return { code: 1, stderr: "GraphQL: API rate limit already exceeded for user ID 1.\n" };
  }
  if (run.startsWith("pr view") && cwd === HOST_LOGIN) {
    const pr = { number: 42, title: "Fix login on devbox", url: "https://github.com/acme/app-ios/pull/42", state: "OPEN", isDraft: false, statusCheckRollup: [], headRefOid: "1".repeat(40), baseRefName: "main" };
    return login === "work" ? { stdout: JSON.stringify(pr) } : hidden;
  }
  if (run.startsWith("api graphql")) return login === "work" ? { stdout: '{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}}' } : hidden;
  return { code: 1, stderr: `no pull requests found for branch "main"\n` };
};
const AUTH_STATUS = "github.com\n  ✓ Logged in to github.com account me (keyring)\n  - Active account: true\n  ✓ Logged in to github.com account work (keyring)\n  - Active account: false\n";
const hostGh = (cwd: string, argv: string[]) => {
  if (argv[0] === "gh") return hostGhAs("me", cwd, argv.slice(1));
  if (argv[0] === "sh" && argv[2]?.startsWith("gh auth status")) return { stdout: AUTH_STATUS };
  if (argv[0] === "sh" && argv[2]?.includes("gh auth token") && argv[4] === "gh") return hostGhAs(["me", "work"].includes(argv[3]) ? argv[3] : "me", cwd, argv.slice(5));
  return null;
};
if (process.env.FIXTURE_HOST_FILES) {
  let statusFails = Number(process.env.FIXTURE_HOST_STATUS_FAILS ?? 0);
  mkdirSync(join(pocket, "hosts"), { recursive: true });
  createServer((c) => {
    let buf = "";
    c.on("data", (raw: Buffer) => {
      // The desktop's websocket to the host's pocketd isn't served here; it retries on its own.
      if (!buf && raw[0] === 0x47) return c.destroy();
      const lines = (buf + raw.toString()).split("\n");
      buf = lines.pop()!;
      for (const line of lines.filter(Boolean)) {
        const op = JSON.parse(line);
        if (process.env.FIXTURE_LOG) appendFileSync(process.env.FIXTURE_LOG.replace(/\.jsonl$/, "-devbox.jsonl"), JSON.stringify({ t: Date.now(), ...op }) + "\n");
        const send = (msg: object) => c.write(JSON.stringify({ id: op.id, ...msg }) + "\n");
        const reply = (msg: object) => (process.env.FIXTURE_HOST_DELAY_MS ? setTimeout(() => send(msg), Number(process.env.FIXTURE_HOST_DELAY_MS)) : send(msg));
        const body = HOST_FILES[op.path];
        let gh: ReturnType<typeof hostGh> = null;
        if (op.op === "status" && statusFails > 0) statusFails--, reply({ ev: "error", error: "pocketd is starting" });
        else if (op.op === "status") reply({ ev: "status", status: { hostId: "devbox", os: "linux", arch: "arm64", userHome: "/home/dev", shell: "/bin/bash", name: "devbox", caps: ["exec.v1", "fs.v1"] } });
        else if (op.op === "fs.list" && Array.isArray(body)) reply({ entries: body.map((n) => hostEntryOf(op.path, n)) });
        else if (op.op === "fs.read" && typeof body === "string") reply({ data: Buffer.from(body).toString("base64") });
        else if (op.op === "fs.stat") reply({ exists: body !== undefined, dir: Array.isArray(body), size: typeof body === "string" ? body.length : 0 });
        else if (op.op === "exec" && op.argv[3] === "clone") {
          if (process.env.FIXTURE_CLONE_DONE) reply({ ev: "exec", stdout: "" });
        } else if (op.op === "exec" && (gh = process.env.FIXTURE_HOST_GH ? hostGh(op.cwd, op.argv) : null)) {
          const { code = 0, stdout = "", stderr = "" } = gh;
          reply({ ev: "exec", code, stdout: Buffer.from(stdout).toString("base64"), stderr: Buffer.from(stderr).toString("base64") });
        } else if (op.op === "exec") {
          const out = hostGit(op.argv);
          reply(out === null ? { ev: "exec", code: 128, stderr: Buffer.from("fatal: not a git repository\n").toString("base64") } : { ev: "exec", stdout: Buffer.from(out).toString("base64") });
        } else if (op.op === "list") reply({ ev: "terminals", items: [] });
        else if (op.op === "accounts" || op.op === "accounts.usage") reply({ ev: "accounts", accounts: [] });
        else reply({ ev: "error", error: `${op.op} isn't in the fixture` });
      }
    });
  }).listen(join(pocket, "hosts", "devbox.sock"));
}

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
// As pocketd's queue: a prompt sent with the queue mode while the agent works waits here, and the next turn takes it once the agent is idle.
// A user's Stop holds the user's entries until their next prompt and drops the notices pocketd sent on its own.
type Entry = { id: string; text: string; origin: string; state: string; order: number };
const queues = new Map<string, Entry[]>();
let entrySeq = 0;
const publish = (ws: Ws, agentId: string, reply?: { type: string; id: string }) => ws.send(JSON.stringify({ type: reply?.type ?? "queue.update", ...(reply && { id: reply.id }), agentId, entries: queues.get(agentId) ?? [] }));
const enqueue = (ws: Ws, agentId: string, text: string) => {
  const list = queues.get(agentId) ?? [];
  list.push({ id: `e${++entrySeq}`, text, origin: "user", state: "pending", order: Math.max(0, ...list.map((e) => e.order)) + 1 });
  queues.set(agentId, list);
  publish(ws, agentId);
};
const take = (ws: Ws, agentId: string, id?: string) => {
  const list = queues.get(agentId) ?? [];
  const next = id ? list.find((e) => e.id === id) : list.filter((e) => e.state === "pending").sort((a, b) => a.order - b.order)[0];
  if (!next) return undefined;
  queues.set(agentId, list.filter((e) => e !== next));
  publish(ws, agentId);
  return next;
};
const release = (ws: Ws, agentId: string) => {
  const list = queues.get(agentId) ?? [];
  if (!list.some((e) => e.state === "held")) return;
  list.forEach((e) => e.state === "held" && (e.state = "pending"));
  publish(ws, agentId);
};
const hold = (ws: Ws, agentId: string) => {
  const list = (queues.get(agentId) ?? []).filter((e) => e.origin === "user");
  list.forEach((e) => (e.state = "held"));
  queues.set(agentId, list);
  publish(ws, agentId);
};
// What a turn does with a prompt once pocketd lets it run.
const takeTurn = (ws: Ws, agentId: string, text: string) => {
  item(ws, agentId, { id: `u-${Date.now()}`, kind: "user", text });
  reply(ws, agentId);
};
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
    const queued = take(ws, agentId);
    if (queued) takeTurn(ws, agentId, queued.text);
  }, 30);
  streams.set(agentId, timer);
};
// As pocketd's orchestrator: the agent delegates three tasks, each a session of its own, and what they report comes back as deliveries in its timeline.
type Task = { taskId: string; parentId: string; childId: string; title: string; agent: string; model?: string; status: string; summary?: string; error?: string; question?: string; createdAt: number; settledAt?: number };
const tasks = new Map<string, Task[]>();
const childTimelines = new Map<string, Item[]>();
const childItems = new Map<string, Item[]>();
const ALL_TASKS: [string, Provider, string, string][] = [
  ["Write the tests", "codex", "gpt-5.5", "Write unit tests for the login redirect in src/auth/redirect.ts."],
  ["Review the diff", "claude", "claude-sonnet-5-5", "Review the diff on this branch and list what could break."],
  ["Update the docs", "opencode", "opencode/big-pickle", "Update docs/auth.md for the new redirect behaviour."],
];
// FIXTURE_DELEGATE_ONE=1: one task with a long title, which finishes; every task of the chat is settled and its panel offers Clear.
const TASKS = process.env.FIXTURE_DELEGATE_ONE ? [["Review the diff on this branch and list what could break", ALL_TASKS[1][1], ALL_TASKS[1][2], ALL_TASKS[1][3]] as (typeof ALL_TASKS)[number]] : ALL_TASKS;
const announce = () => conns.forEach((c) => c.write(JSON.stringify({ ev: "terminals", items: sessions }) + "\n"));
const setTask = (ws: Ws, t: Task, patch: Partial<Task>) => {
  Object.assign(t, patch);
  const child = summaries.find((s) => s.id === t.childId);
  if (child) {
    const phase = t.status === "completed" ? "done" : t.status === "failed" ? "done" : t.question ? "needsYou" : "working";
    Object.assign(child, { status: phase, failed: t.status === "failed", delegation: { taskId: t.taskId, status: t.status, workState: phase }, updatedAt: Date.now() });
    ws.send(JSON.stringify({ type: "agent.update", agent: child }));
    const ended: Item[] =
      t.status === "completed"
        ? [{ id: `${t.childId}-s`, kind: "assistant", text: t.summary ?? "" }, { id: `${t.childId}-r`, kind: "result", ok: true, durationMs: 1000, usage: { inputTokens: 9000, outputTokens: 400 } }]
        : t.status === "failed"
          ? [{ id: `${t.childId}-r`, kind: "result", ok: false, durationMs: 1000, error: t.error }]
          : [];
    for (const it of ended) item(ws, t.childId, it);
    // A child opened later reads its timeline, which holds how it ended.
    if (ended.length) childTimelines.set(t.childId, stamp(t.childId, [...childItems.get(t.childId)!, ...ended]));
  }
  ws.send(JSON.stringify({ type: "delegation.update", agentId: t.parentId, delegation: t }));
};
const WAKES: Record<string, string> = {
  "Which test runner?": "**Write the tests** asks which test runner to use. That one is yours; I left it waiting.",
  "Two findings": "**Review the diff** found two things: an unused import in `src/auth/redirect.ts`, and a missing check on `next`. Both are fixed on its branch.",
  "opencode exited 2: provider returned 401.": "**Update the docs** failed: OpenCode's provider returned 401. I'll redo the docs myself once the tests are in.",
  "24 tests pass": "All three are back. **24 tests pass** on Vitest, the review fixed an unused import and a missing check on `next`, and the docs task failed on an expired OpenCode key, so I'll update `docs/auth.md` myself next.",
};
const deliver = (ws: Ws, t: Task, outcome: "done" | "failed" | "asks") => {
  item(ws, t.parentId, { id: `delivery-${t.taskId}-${outcome}`, kind: "delivery", taskIds: [t.taskId], childIds: [t.childId], titles: [t.title], agents: [t.agent], outcome, ...(outcome === "asks" && { question: t.question }) });
  const said = WAKES[(outcome === "asks" ? t.question : outcome === "failed" ? t.error : t.summary) ?? ""];
  if (process.env.FIXTURE_DELEGATE_WAKE && said) setTimeout(() => reply(ws, t.parentId, said, t.summary === "24 tests pass" ? 60 : 8), 20);
};
// pocketd's default isolation: a child gets a worktree of its own from the parent's checkout; FIXTURE_DELEGATE_SHARED=1 keeps it in the parent's, as isolation "shared" does.
const childCheckout = (parentId: string, parentCwd: string, title: string) => {
  const local = (sessions.find((s) => s.id === parentId) as { local?: string } | undefined)?.local ?? "";
  if (process.env.FIXTURE_DELEGATE_SHARED) return { cwd: parentCwd, local };
  const common = Bun.spawnSync(["git", "-C", parentCwd, "rev-parse", "--path-format=absolute", "--git-common-dir"]).stdout.toString().trim();
  const repo = dirname(common);
  const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  const tree = join(`${repo}.worktrees`, `delegate-${slug}`);
  git(repo, "worktree", "add", "-q", "-b", `delegate/${slug}`, tree, "HEAD");
  return { cwd: tree, local: "" };
};
// A child with two turns of several paragraphs each, for screens of a delegated chat scrolled under its breadcrumb.
const longChild = (id: string): Item[] => {
  const say = (turn: string, n: number): Item[] => Array.from({ length: n }, (_, i) => ({ id: `${id}-${turn}${i}`, kind: "assistant", text: `Paragraph ${i + 1}: read the redirect helper, traced where next is parsed, and checked each caller for an unvalidated target.` }) as Item);
  return [...say("p", 6), { id: `${id}-r1`, kind: "result", ok: true, durationMs: 1000 }, { id: `${id}-u2`, kind: "user", text: "Now check the cookie options too" }, ...say("q", 6)];
};
const delegate = (ws: Ws, parentId: string) => {
  const parent = summaries.find((s) => s.id === parentId);
  if (!parent) return;
  const list: Task[] = [];
  tasks.set(parentId, list);
  TASKS.forEach(([title, agent, model, brief], i) => {
    const taskId = `${parentId}-task${i + 1}`;
    const childId = `${parentId}-c${i + 1}`;
    setTimeout(() => {
      const t: Task = { taskId, parentId, childId, title, agent, model, status: "starting", createdAt: Date.now() };
      list.push(t);
      const { cwd, local } = childCheckout(parentId, parent.cwd, title);
      sessions.push({ id: childId, cmd: agent, args: [], cwd, local });
      childItems.set(childId, [{ id: `${childId}-brief`, kind: "user", text: brief }, { id: `${childId}-a`, kind: "assistant", text: "On it." }, ...(process.env.FIXTURE_CHILD_LONG ? longChild(childId) : [])]);
      childTimelines.set(childId, stamp(childId, childItems.get(childId)!));
      summaries.push({ ...parent, id: childId, terminalId: childId, cwd, title, provider: agent, model, effort: undefined, status: "working", failed: false, pinned: false, chatOnly: true, parent: parentId, relationship: "subagent", createdBy: `agent:${parentId}`, delegation: { taskId, status: "starting", workState: "working" }, createdAt: Date.now(), updatedAt: Date.now() } as (typeof summaries)[number]);
      announce();
      ws.send(JSON.stringify({ type: "agent.update", agent: summaries.at(-1) }));
      ws.send(JSON.stringify({ type: "delegation.update", agentId: parentId, delegation: t }));
    }, 250 + i * 150);
  });
  const at = (ms: number, f: () => void) => setTimeout(f, ms);
  at(900, () => list.forEach((t) => setTask(ws, t, { status: "working" })));
  if (process.env.FIXTURE_DELEGATE_ONE) {
    at(1700, () => {
      setTask(ws, list[0], { status: "completed", summary: "Two findings", settledAt: Date.now() });
      deliver(ws, list[0], "done");
    });
    return;
  }
  at(1300, () => {
    setTask(ws, list[0], { question: "Which test runner?" });
    deliver(ws, list[0], "asks");
  });
  at(1700, () => {
    setTask(ws, list[1], { status: "completed", summary: "Two findings", settledAt: Date.now() });
    deliver(ws, list[1], "done");
  });
  at(2100, () => {
    setTask(ws, list[2], { status: "failed", error: "opencode exited 2: provider returned 401.", settledAt: Date.now() });
    deliver(ws, list[2], "failed");
  });
  if (process.env.FIXTURE_DELEGATE_WAKE)
    at(2800, () => {
      setTask(ws, list[0], { status: "completed", question: undefined, summary: "24 tests pass", settledAt: Date.now() });
      deliver(ws, list[0], "done");
    });
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
        ws.send(JSON.stringify({ type: "hello.ok", id: f.id, serverId: "fixture", hostname: "fixture", protocolVersion: 3, caps: ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "automations.v1", "board.v1", "locals.v1", "settings.v1", "access.v1", ...(process.env.FIXTURE_NO_ORCHESTRATOR ? [] : ["orchestrator.v1"]), ...(process.env.FIXTURE_NO_HOSTS ? [] : ["hosts.v1"])], protocol: { min: 3, max: 3 }, scopes: SCOPES, host: { tailnet: false, keepingAwake: true } }));
        ws.send(JSON.stringify({ type: "agent.list", agents: summaries }));
        ws.send(JSON.stringify({ type: "automations", automations: AUTOMATIONS, runs: RUNS }));
        ws.send(JSON.stringify({ type: "board", tickets: TICKETS, limit: 4 }));
        if (!process.env.FIXTURE_NO_HOSTS) ws.send(JSON.stringify({ type: "host.list", hosts }));
        for (const a of scenario.agents.filter((a) => a.waiting)) {
          ws.send(JSON.stringify({ type: "permission.request", request: { requestId: `ask-${a.id}`, agentId: a.id, toolName: "Bash", detail: { kind: "shell", command: a.waiting } } }));
        }
      }
      if (f.type.startsWith("host.")) hostVerb(ws, f);
      if (f.type === "pair.begin") {
        const code = "abcdefghijklmnopqrstuv";
        ws.send(JSON.stringify({ type: "pair.offer", id: f.id, url: `anywhere://pair?v=1&h=100.64.0.1:4517&c=${code}`, code, expiresAt: Date.now() + 300_000 }));
      }
      // FIXTURE_HOST_ONLY: every chat has three tasks whose sessions run on linked hosts, so none is in agent.list.
      // FIXTURE_HOST_DOWN=1 tags them: the second runs on devbox (online), the third on 10.0.4.12 (retrying).
      const hostOnly = (id: string): Task[] => process.env.FIXTURE_HOST_ONLY ? TASKS.map(([title, agent, model], i) => ({ taskId: `${id}-host${i + 1}`, parentId: id, childId: `${process.env.FIXTURE_HOST_DOWN ? (i === 2 ? "10-0-4-12/" : "devbox/") : ""}${id}-host-c${i + 1}`, title, agent, model, status: "working", ...(i === 0 && { question: "Which test runner?" }), createdAt: Date.now() + i })) : [];
      if (f.type === "agent.delegations") ws.send(JSON.stringify({ type: "agent.delegations", id: f.id, agentId: f.agentId, items: tasks.get(f.agentId) ?? hostOnly(f.agentId) }));
      if (f.type === "agent.timeline" && childTimelines.has(f.agentId)) ws.send(JSON.stringify({ type: "agent.timeline", agentId: f.agentId, items: childTimelines.get(f.agentId) }));
      if (f.type === "agent.timeline") {
        const a = scenario.agents.find((a) => a.id === f.agentId);
        if (a) ws.send(JSON.stringify({ type: "agent.timeline", agentId: a.id, items: timeline(a) }));
        if (a?.id === CHAT_AGENT && firstChat.at(-1)?.kind === "user") reply(ws, a.id);
        if (a?.id === CHAT_AGENT && process.env.FIXTURE_CHAT === "permissions") ask(ws, a.id, "permission");
      }
      if (f.type === "agent.prompt" && f.queue && streams.has(f.agentId)) enqueue(ws, f.agentId, f.text);
      else if (f.type === "agent.prompt") {
        release(ws, f.agentId);
        const name = f.text.match(/^\/fixture (\S+)$/)?.[1];
        if (name === "permission" || name === "question") ask(ws, f.agentId, name);
        else if (name === "exit") exit(ws, f.agentId);
        else if (name === "clear") clear(ws, f.agentId);
        else if (name === "restart") restart(ws, f.agentId);
        else if (name === "stream-code") reply(ws, f.agentId, CODE_REPLY, 150);
        else if (name === "delegate") {
          item(ws, f.agentId, { id: `u-${Date.now()}`, kind: "user", text: f.text });
          reply(ws, f.agentId, "I'll split this three ways: tests on Codex, a review of the diff on Claude, and the docs on OpenCode. Each runs as a session of its own, and what it reports comes back here.", 6);
          delegate(ws, f.agentId);
        }
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
      if (f.type === "queue.list") publish(ws, f.agentId, { type: "queue.list", id: f.id });
      if (process.env.FIXTURE_REFUSE_QUEUE && /^queue\.(edit|cancel|reorder|send_now)$/.test(f.type)) return ws.send(JSON.stringify({ type: "error", id: f.id, message: "That queued message was sent or removed" }));
      if (f.type === "queue.edit") {
        const e = queues.get(f.agentId)?.find((e) => e.id === f.entryId);
        if (e) e.text = f.text;
        publish(ws, f.agentId);
      }
      if (f.type === "queue.cancel") {
        queues.set(f.agentId, (queues.get(f.agentId) ?? []).filter((e) => e.id !== f.entryId));
        publish(ws, f.agentId);
      }
      if (f.type === "queue.reorder") {
        const list = queues.get(f.agentId) ?? [];
        const e = list.find((e) => e.id === f.entryId);
        const rest = list.filter((x) => x !== e).sort((a, b) => a.order - b.order);
        const at = f.before ? rest.findIndex((x) => x.id === f.before) : -1;
        if (e) e.order = at < 0 ? Math.max(0, ...rest.map((x) => x.order)) + 1 : at === 0 ? rest[0].order - 1 : (rest[at - 1].order + rest[at].order) / 2;
        queues.set(f.agentId, list.sort((a, b) => a.order - b.order));
        publish(ws, f.agentId);
      }
      if (f.type === "queue.send_now") {
        const e = take(ws, f.agentId, f.entryId);
        if (e) {
          clearInterval(streams.get(f.agentId));
          streams.delete(f.agentId);
          takeTurn(ws, f.agentId, e.text);
        }
      }
      if (f.type === "agent.commands") {
        const commands = [
          { name: "review", description: "Review a pull request", argumentHint: "<pr>" },
          { name: "release-notes", description: "Draft release notes from the merged pull requests" },
          { name: "goal", description: "Set a goal — keep working until the condition is met" },
          {
            name: "auto-mode-setup",
            description: "Set up auto mode",
            argumentHint: "[--request-id <uuid>] (--wizard posture=… scope=… depth=… --propose | --expect-sha256 <64-hex> --apply-file <path>)",
          },
        ];
        ws.send(JSON.stringify({ type: "agent.commands", id: f.id, agentId: f.agentId, commands }));
      }
      if (f.type === "agent.mcp") {
        const servers = [
          { name: "fff", status: "connected", scope: "user" },
          { name: "github", status: "pending", scope: "project" },
          { name: "linear", status: "needsAuth", scope: "claude.ai" },
          { name: "figma", status: "failed", scope: "user" },
          { name: "sentry", status: "disabled", scope: "local" },
        ];
        ws.send(JSON.stringify({ type: "agent.mcp", id: f.id, agentId: f.agentId, servers }));
      }
      if (f.type === "agent.remote_control") {
        const url = f.enabled ? { url: "https://claude.ai/code/session_fixture" } : {};
        ws.send(JSON.stringify({ type: "agent.remote_control", id: f.id, agentId: f.agentId, ...url }));
      }
      // A session from the New session page: its terminal and agent appear, then the agent answers the prompt.
      if (f.type === "agent.create") {
        const { project, checkout, provider, prompt } = f.spec;
        const id = `new-${f.requestId}`;
        const tree = checkout?.new?.name ? join(`${project}.worktrees`, checkout.new.name) : project;
        mkdirSync(tree, { recursive: true });
        sessions.push({ id, cmd: provider, args: [], cwd: tree, local: checkout?.local ?? "" });
        summaries.push({ ...summaries[0], id, terminalId: id, title: prompt ?? "New session", cwd: tree, provider, status: "working", pinned: false, chatOnly: !!f.spec.chat, createdAt: Date.now(), updatedAt: Date.now() });
        ws.send(JSON.stringify({ type: "agent.creating", id: f.id, requestId: f.requestId, terminalId: id, cwd: tree, setup: false }));
        ws.send(JSON.stringify({ type: "agent.list", agents: summaries }));
        ws.send(JSON.stringify({ type: "agent.created", id: f.id, requestId: f.requestId, agentId: id }));
        if (prompt) {
          setTimeout(() => {
            item(ws, id, { id: `u-${Date.now()}`, kind: "user", text: prompt });
            reply(ws, id);
          }, Number(process.env.FIXTURE_ECHO_MS) || 0);
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
          hold(ws, f.agentId);
        }, Number(process.env.FIXTURE_STOP_MS) || 0);
      }
      if (f.type === "agent.providers" && (process.env.FIXTURE_SEND_NOW || name.startsWith("agents"))) {
        const chat = { interrupt: true, compact: true, sendWhileWorking: true, subAgents: true, sendNow: !!process.env.FIXTURE_SEND_NOW };
        const claude = { id: "claude", available: true, efforts: ["low", "medium", "high", "xhigh", "max"], plan: true, chat };
        const providers = name.startsWith("agents") ? [claude, ...PROVIDER_IDS.slice(1).map(provider)] : [claude];
        ws.send(JSON.stringify({ type: "agent.providers", id: f.id, providers, maxAccess: "full", phoneMaxAccess: "ask" }));
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
