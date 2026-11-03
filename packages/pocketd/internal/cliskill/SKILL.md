---
name: anywhere
description: Run and follow Claude Code and Codex sessions in Anywhere, the user's desktop app for coding agents, with the `anywhere` command. Use when the user asks to start a session in Anywhere, hand work to another agent there, send a session a prompt, check on sessions, or read what one replied.
---

# anywhere

`anywhere` acts as the user on this Mac. Every session it starts shows in the Anywhere app like one the user opened.

## Commands

```sh
anywhere project list [--json]
anywhere session list [--json]
anywhere session create [--project <name|path>] [--worktree <name> | --new-worktree [<name>] [--base <branch>]] [--provider claude|codex] [--access settings|ask|edits|auto|full] [--prompt <text>]
anywhere session prompt <id> <text|->
anywhere session wait [--timeout 30m] <id>
anywhere session read [--limit 200] [--json] <id>
anywhere session close <id>
```

`create` prints the session's id. Without `--project` it takes the project holding the current folder. Without a worktree flag it runs in the project's main worktree, the same folder the user works in. `--new-worktree` makes a worktree from the default branch (`main`), or from `--base`; without a name, Anywhere names it from the prompt, as the app does. `prompt <id> -` reads the text from stdin.

## Hand over a task

```sh
id=$(anywhere session create --new-worktree --prompt "Fix the login redirect")
anywhere session wait "$id"
anywhere session read "$id"
```

`create --prompt` and `prompt` return once the turn has started, so `wait` right after them waits for that turn.

`wait` prints one word and exits with its code:

| Exit | Word | What to do |
|---|---|---|
| 0 | `done` | `read` the reply |
| 1 | | The command failed; stderr says why. If the connection dropped, run `wait` again |
| 2 | | Fix the command line |
| 3 | `failed` | `read` the reply, tell the user |
| 4 | `needsYou` | The agent is waiting on the user. Tell them, then run `wait` again |
| 5 | `timeout` | The agent is still working. Run `wait` again or tell the user |
| 6 | `closed` | The session is gone |

## Rules

- Use `--new-worktree` when sessions run side by side, or when the user may be working in the main worktree.
- Leave `--access` out unless the user asks for a level. Pass `full` only when they ask for it by name.
- Permission prompts are the user's. Never try to answer one.
- Don't close a session you didn't create unless the user asks.
- If `anywhere` says a process inside an Anywhere terminal can only read sessions, you are running inside Anywhere. Don't retry; tell the user.
