#!/usr/bin/env python3
"""Proves an AskUserQuestion is answered from the notch island, against a real Claude and this worktree's pocketd and desktop.

Claude asks two questions, one single-select and one multi-select. The island shows them; the answers go back through
pocketd as the hook's updatedInput, and Claude replies with what it got:
  Which color? → Blue · Which sizes? → S, L   → Claude's reply has `color=Blue; sizes=S, L`
With `person`, you answer on the island: hover it, click Blue, tick S and L, click Submit.
Without it, the script answers over pocketd's socket with the frame the desktop sends (Outbox::answer), so it runs unattended.
Screenshots of the island land in .ui-review/shots/questions-proof/out. Run from the repo root:
  python3 -I .ui-review/notch-proof/questions.py [person]
"""
import json, os, shutil, signal, subprocess, sys, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import prove  # noqa: E402

SCRATCH = os.path.join(prove.ROOT, ".ui-review", "shots", "questions-proof")
# Claude runs pocketd's hooks only in a repo it trusts, and trust stops at a repo's toplevel, so a fresh scratch repo
# gets none; this worktree is trusted, and Claude only asks questions in it.
prove.SCRATCH, prove.REPO, prove.OUT = SCRATCH, prove.ROOT, f"{SCRATCH}/out"
# A unix socket path must stay under 104 bytes.
prove.HOME = os.path.expanduser("~/.cache/questions-proof")
prove.SOCK = f"{prove.HOME}/pocketd.sock"
log = prove.log

PROMPT = ("Use the AskUserQuestion tool exactly once, with two questions: "
          "'Which color?' (header 'Color', options Red and Blue, single select) and "
          "'Which sizes?' (header 'Sizes', options S, M and L, multiSelect true). "
          "Then reply with exactly `color=<answer>; sizes=<answer>` using the answers you got, nothing else.")


def answers_for(questions):
    color, sizes = questions
    return {color["question"]: "Blue", sizes["question"]: "S, L"}


def run(ws, check, person, pid):
    agent = prove.start(ws, "questions", PROMPT)
    if not agent:
        return check("the session started", False)
    ask = prove.wait_for(ws, lambda f: prove.asked(agent)(f) and f["request"]["toolName"] == "AskUserQuestion", 600)
    if not ask:
        return check("Claude asked with AskUserQuestion", False)
    req = ask["request"]
    with open(f"{prove.OUT}/permission_request.json", "w") as f:
        json.dump(ask, f, indent=2, ensure_ascii=False)
    questions = (req["detail"].get("input") or {}).get("questions") or []
    check("the ask carries both questions with their options",
          [[o["label"] for o in q["options"]] for q in questions] == [["Red", "Blue"], ["S", "M", "L"]]
          and [q.get("multiSelect", False) for q in questions] == [False, True])
    if len(questions) != 2:
        return
    if person:
        log("ASK: hover the island → click Blue → tick S and L → Submit")
    else:
        time.sleep(2)  # Long enough for the island to show it.
        x, y, w, _ = prove.island_bounds(pid)[0]
        subprocess.run(["cliclick", f"m:{x + w // 2},{y + 15}"])  # Opens the island, for the screenshots.
        time.sleep(3)
        ws.send({"type": "permission.resolve", "id": "resolve", "requestId": req["requestId"], "decision": "allow", "answers": answers_for(questions)})
    r = prove.wait_for(ws, prove.resolved(req["requestId"]), 600)
    check("the ask resolved as allow", bool(r) and r.get("decision") == "allow")

    reply = None
    for _ in range(30):
        time.sleep(2)
        ws.send({"type": "agent.timeline", "id": "tl", "agentId": agent, "limit": 200})
        tl = prove.wait_for(ws, lambda f: f.get("type") == "agent.timeline" and f.get("id") == "tl", 20) or {}
        texts = [i.get("text", "") for i in tl.get("items", []) if i.get("kind") != "user"]
        reply = next((t for t in texts if "color=" in t), None)
        if reply:
            break
    with open(f"{prove.OUT}/timeline.json", "w") as f:
        json.dump(tl, f, indent=2, ensure_ascii=False)
    log("Claude replied:", reply)
    check("Claude got the answers: color=Blue; sizes=S, L", bool(reply) and "color=Blue" in reply and "sizes=S, L" in reply)


def setup():
    for d in (prove.HOME, SCRATCH):
        shutil.rmtree(d, ignore_errors=True)
    for d in (prove.HOME, prove.OUT):
        os.makedirs(d)
    with open(f"{prove.HOME}/config.json", "w") as f:
        json.dump({"port": 4597, "listen": "loopback", "restore": {"resumeAgents": False}}, f)
    with open(f"{prove.HOME}/desktop.json", "w") as f:
        json.dump({"projects": [prove.REPO], "notifications": {"on_screen": True}}, f)
    log("building pocketd and the release desktop")
    subprocess.run(["go", "build", "-o", f"{SCRATCH}/pocketd", "./cmd/pocketd"], cwd=f"{prove.ROOT}/packages/pocketd", check=True)
    subprocess.run(["cargo", "build", "--release", "-p", "pocket"], cwd=f"{prove.ROOT}/packages/desktop", check=True, capture_output=True)
    subprocess.run(["swift", "build", "-c", "release"], cwd=f"{prove.ROOT}/packages/desktop/notch", check=True, capture_output=True)


def main():
    person = sys.argv[1:] == ["person"]
    setup()
    env = prove.clean_env()
    daemon = subprocess.Popen([f"{SCRATCH}/pocketd", "serve"], env=env, stdout=open(f"{prove.OUT}/pocketd.log", "w"), stderr=subprocess.STDOUT, start_new_session=True)
    while not os.path.exists(prove.SOCK):
        time.sleep(0.2)
    desktop = subprocess.Popen([f"{prove.ROOT}/packages/desktop/target/release/pocket-desktop"], env=env, stdout=open(f"{prove.OUT}/desktop.log", "w"), stderr=subprocess.STDOUT)
    stop = threading.Event()
    threading.Thread(target=prove.watch, args=(desktop.pid, stop), daemon=True).start()
    results = []
    check = lambda name, ok: (results.append((name, ok)), log(("PASS" if ok else "FAIL"), name))
    try:
        ws = prove.WS(prove.SOCK)
        ws.send({"type": "hello", "id": "h", "clientId": "questions-proof", "protocolVersion": 3, "caps": ["launch.v1", "summary.v2"]})
        run(ws, check, person, desktop.pid)
        time.sleep(3)
    finally:
        stop.set()
        desktop.send_signal(signal.SIGTERM)
        os.killpg(daemon.pid, signal.SIGTERM)
        passed = bool(results) and all(ok for _, ok in results)
        log("RESULT", "PASS" if passed else "FAIL", f"· screenshots and frames in {prove.OUT}")
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
