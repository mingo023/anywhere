#!/usr/bin/env python3
"""Proves the notch island against a real Claude session, away from the installed Anywhere and any dev pocketd.

Starts a scratch pocketd and this worktree's release desktop, has Claude ask to run two commands,
screenshots the island (the `anywhere-notch` helper's panel) each time it looks different, and checks what each answer did:
  ask 1 · Allow            → ~/.cache/notch-proof/allowed exists
  ask 2 · Reply instead…   → ~/.cache/notch-proof/denied doesn't, and the reply is Claude's next prompt
With `two`, two sessions ask at once instead, and the island queues them:
  touch …/allow · Allow           → ~/.cache/notch-proof/allow exists
  touch …/deny  · Deny and stop   → ~/.cache/notch-proof/deny doesn't
It sends no keyboard or mouse input: a person answers the island. Run from the repo root:
  python3 -I .ui-review/notch-proof/prove.py [two]
"""
import base64, json, os, select, shutil, signal, socket, struct, subprocess, sys, threading, time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
HERE = os.path.dirname(os.path.abspath(__file__))
SCRATCH = os.path.expanduser("~/.cache/notch-proof")
# Claude runs pocketd's hooks, which tell it about asks, only in a git repo ~/.claude.json trusts: run `claude` there once and accept.
HOME, REPO, OUT = f"{SCRATCH}/home", f"{SCRATCH}/repo", f"{SCRATCH}/out"
SOCK = f"{HOME}/pocketd.sock"
REPLY = "Don't create that file. Just answer ok."
PROMPT = (f"Run `touch {SCRATCH}/allowed` with the Bash tool. "
          f"Then, as a separate Bash call, run `touch {SCRATCH}/denied`. Nothing else.")


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, flush=True)


class WS:
    """Just enough of a websocket client for pocketd's socket."""

    def __init__(self, path):
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(path)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall(f"GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                       f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self.s.recv(4096)
        head, self.buf = buf.split(b"\r\n\r\n", 1)
        # Frames read while waiting for another, kept for a later wait.
        self.backlog = []
        assert b" 101 " in head.split(b"\r\n")[0], head

    def send(self, obj):
        self._frame(0x1, json.dumps(obj).encode())

    def _frame(self, op, data):
        mask, n = os.urandom(4), len(data)
        size = bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack(">H", n) if n < 65536 else bytes([0x80 | 127]) + struct.pack(">Q", n)
        self.s.sendall(bytes([0x80 | op]) + size + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def _read(self, n):
        while len(self.buf) < n:
            chunk = self.s.recv(65536)
            if not chunk:
                raise EOFError
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def recv(self, timeout):
        """The next text message, or None after `timeout` seconds."""
        if not self.buf and not select.select([self.s], [], [], timeout)[0]:
            return None
        msg = b""
        while True:
            b0, b1 = self._read(2)
            n = b1 & 0x7F
            n = struct.unpack(">H", self._read(2))[0] if n == 126 else struct.unpack(">Q", self._read(8))[0] if n == 127 else n
            payload = self._read(n)
            op = b0 & 0x0F
            if op == 8:
                raise EOFError
            if op == 9:
                self._frame(0xA, payload)
            if op in (9, 10):
                if not msg:
                    return None
                continue
            msg += payload
            if b0 & 0x80:
                return json.loads(msg)


def island_bounds(desktop):
    """The pop-up panels (window level 101) the desktop's `anywhere-notch` helper has on screen, as (x, y, w, h)."""
    pid = subprocess.run(["pgrep", "-P", str(desktop), "-x", "anywhere-notch"], capture_output=True, text=True).stdout.split()
    if not pid:
        return []
    pid = pid[0]
    out = subprocess.run(["osascript", "-l", "JavaScript", f"{HERE}/windows.js", str(pid)], capture_output=True, text=True).stdout.strip()
    rows = [tuple(map(int, r.split(","))) for r in out.split(";") if r]
    return [r[1:] for r in rows if r[0] == 101]


def watch(pid, stop):
    """Screenshots the island each time it looks different: compact, open, typing, answered."""
    last, n = None, 0
    while not stop.is_set():
        b = island_bounds(pid)
        if b:
            x, y, w, h = b[0]
            subprocess.run(["screencapture", "-x", "-R", f"{x},{y},{w},{h}", f"{OUT}/now.png"])
            with open(f"{OUT}/now.png", "rb") as f:
                shot = f.read()
            if shot != last:
                last, n = shot, n + 1
                os.rename(f"{OUT}/now.png", f"{OUT}/{n:02d}.png")
                log(f"island changed → {OUT}/{n:02d}.png")
        time.sleep(0.5)


def clean_env():
    env = {k: v for k, v in os.environ.items() if not k.startswith(("POCKETD_", "CLAUDE_CODE_", "CLAUDECODE"))}
    env["POCKET_HOME"] = HOME
    return env


def setup():
    shutil.rmtree(SCRATCH, ignore_errors=True)
    for d in (HOME, REPO, OUT):
        os.makedirs(d)
    with open(f"{HOME}/config.json", "w") as f:
        json.dump({"port": 4597, "listen": "loopback", "restore": {"resumeAgents": False}}, f)
    # pocketd launches only into projects the desktop lists here.
    with open(f"{HOME}/desktop.json", "w") as f:
        # Islands for a session in a pane on screen too: the desktop may well show this one.
        json.dump({"projects": [REPO], "notifications": {"on_screen": True}}, f)
    git = lambda *a: subprocess.run(["git", "-C", REPO, *a], check=True, capture_output=True)
    git("init", "-b", "main")
    with open(f"{REPO}/README.md", "w") as f:
        f.write("notch proof\n")
    git("add", ".")
    git("-c", "user.name=proof", "-c", "user.email=proof@example.com", "commit", "-m", "init")
    log("building pocketd and the release desktop")
    subprocess.run(["go", "build", "-o", f"{SCRATCH}/pocketd", "./cmd/pocketd"], cwd=f"{ROOT}/packages/pocketd", check=True)
    subprocess.run(["cargo", "build", "--release", "-p", "pocket"], cwd=f"{ROOT}/packages/desktop", check=True, capture_output=True)
    subprocess.run(["swift", "build", "-c", "release"], cwd=f"{ROOT}/packages/desktop/notch", check=True, capture_output=True)


def wait_for(ws, want, timeout):
    """The first frame, read before or now, that `want`s; or None after `timeout`."""
    for f in ws.backlog:
        if want(f):
            ws.backlog.remove(f)
            return f
    end = time.time() + timeout
    while time.time() < end:
        f = ws.recv(1)
        if f is None:
            continue
        if want(f):
            return f
        ws.backlog.append(f)
    return None


def start(ws, name, prompt):
    """Starts Claude on `prompt`; its agent id, or None."""
    spec = {"project": REPO, "checkout": {"worktree": REPO}, "provider": "claude", "access": "ask", "plan": False, "prompt": prompt}
    ws.send({"type": "agent.create", "id": name, "requestId": name, "spec": spec})
    created = wait_for(ws, lambda f: f.get("type") in ("agent.created", "error") and f.get("id") == name, 120)
    if not created or created["type"] != "agent.created":
        return log("couldn't start Claude:", created)
    log("Claude started:", created["agentId"])
    return created["agentId"]


asked = lambda agent, but=None: lambda f: (f.get("type") == "permission.request" and f["request"]["agentId"] == agent
                                           and f["request"]["requestId"] != but)
resolved = lambda rid: lambda f: f.get("type") == "permission.resolved" and f.get("requestId") == rid


def one(ws, check):
    agent = start(ws, "proof", PROMPT)
    if not agent:
        return check("the session started", False)
    ask = wait_for(ws, asked(agent), 600)
    if not ask:
        return check("the first ask came", False)
    first = ask["request"]["requestId"]
    log("ASK 1:", ask["request"]["detail"].get("command"), "→ hover the island, then click Allow")
    r = wait_for(ws, resolved(first), 600)
    check("ask 1 resolved as allow", bool(r) and r.get("decision") == "allow")

    ask = wait_for(ws, asked(agent, but=first), 180)
    if not ask:
        return check("the second ask came", False)
    log("ASK 2:", ask["request"]["detail"].get("command"), f"→ hover, click Reply instead…, type “{REPLY}”, press Return")
    r = wait_for(ws, resolved(ask["request"]["requestId"]), 600)
    check("ask 2 resolved as deny", bool(r) and r.get("decision") == "deny")
    time.sleep(15)
    ws.send({"type": "agent.timeline", "id": "tl", "agentId": agent, "limit": 200})
    tl = wait_for(ws, lambda f: f.get("type") == "agent.timeline" and f.get("id") == "tl", 20) or {}
    prompts = [i.get("text", "") for i in tl.get("items", []) if i.get("kind") == "user"]
    check("allowed command ran", os.path.exists(f"{SCRATCH}/allowed"))
    check("denied command didn't run", not os.path.exists(f"{SCRATCH}/denied"))
    check("the reply became Claude's next prompt", any(REPLY in p for p in prompts))
    log("user prompts in the timeline:", prompts)


def two(ws, check):
    agents = {name: start(ws, name, f"Run `touch {SCRATCH}/{name}` with the Bash tool. Nothing else.") for name in ("allow", "deny")}
    if None in agents.values():
        return check("both sessions started", False)
    asks = {}
    for name, agent in agents.items():
        ask = wait_for(ws, asked(agent), 600)
        if not ask:
            return check(f"the {name} session asked", False)
        asks[name] = ask["request"]["requestId"]
    check("both asks waited at once", not any(resolved(q)(f) for q in asks.values() for f in ws.backlog))
    log("ASKS: the island says Needs you · 2 → Allow `touch …/allow`, then Deny and stop `touch …/deny`, in the order they show")
    for name in asks:
        r = wait_for(ws, resolved(asks[name]), 600)
        check(f"the {name} session's ask resolved as {name}", bool(r) and r.get("decision") == name)
    time.sleep(10)
    check("the allowed session's command ran", os.path.exists(f"{SCRATCH}/allow"))
    check("the denied session's command didn't run", not os.path.exists(f"{SCRATCH}/deny"))


def main():
    scenario = two if sys.argv[1:] == ["two"] else one
    setup()
    env = clean_env()
    daemon = subprocess.Popen([f"{SCRATCH}/pocketd", "serve"], env=env, stdout=open(f"{OUT}/pocketd.log", "w"), stderr=subprocess.STDOUT, start_new_session=True)
    while not os.path.exists(SOCK):
        time.sleep(0.2)
    desktop = subprocess.Popen([f"{ROOT}/packages/desktop/target/release/pocket-desktop"], env=env, stdout=open(f"{OUT}/desktop.log", "w"), stderr=subprocess.STDOUT)
    stop = threading.Event()
    threading.Thread(target=watch, args=(desktop.pid, stop), daemon=True).start()
    results = []
    check = lambda name, ok: (results.append((name, ok)), log(("PASS" if ok else "FAIL"), name))
    try:
        ws = WS(SOCK)
        ws.send({"type": "hello", "id": "h", "clientId": "notch-proof", "protocolVersion": 3, "caps": ["launch.v1", "summary.v2"]})
        scenario(ws, check)
        time.sleep(3)
    finally:
        stop.set()
        desktop.send_signal(signal.SIGTERM)
        os.killpg(daemon.pid, signal.SIGTERM)
        passed = bool(results) and all(ok for _, ok in results)
        log("RESULT", "PASS" if passed else "FAIL", f"· screenshots in {OUT}")
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
