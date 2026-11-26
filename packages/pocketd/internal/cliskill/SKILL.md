---
name: zoes
description: Run and follow coding agent sessions (Claude Code, Codex and the other agents it runs) in Zoes, the user's desktop app for coding agents, with the `zoes` command. Use when the user asks to start a session in Zoes, manage tickets on its Board, hand work to another agent there, send a session a prompt, check on sessions, or read what one replied.
---

# zoes

`zoes` acts as the user on this Mac. Every session it starts shows in the Zoes app like one the user opened. Zoes was called Anywhere; `anywhere` is the same command and still works.

## Commands

```sh
zoes project list [--json]
zoes session list [--json]
zoes session create [--project <name|path>] [--worktree <name> | --new-worktree [<name>] [--base <branch>]] [--provider <agent>] [--access settings|ask|edits|auto|full] [--prompt <text>]
zoes session prompt <id> <text|->
zoes session wait [--timeout 30m] <id>
zoes session read [--limit 200] [--json] <id>
zoes session close <id>
zoes ticket create [--project <name|path>] [--provider <agent>] [--base <branch>] [--title <text>] [--start] <prompt|->
zoes ticket list [--json]
zoes ticket read [--json] <id>
zoes ticket update [--title <text>] [--provider <agent>] [--base <branch>] [--prompt <text|->] <id>
zoes ticket delete <id>
```

`<agent>` is `claude` when left out; `zoes` run with no arguments lists the others it can start. `create` prints the session's id. Without `--project` it takes the project holding the current folder. Without a worktree flag it runs in the project's main worktree, the same folder the user works in. `--new-worktree` makes a worktree from the default branch (`main`), or from `--base`; without a name, Zoes names it from the prompt, as the app does. `prompt <id> -` reads the text from stdin.

## Hand over a task

```sh
id=$(zoes session create --new-worktree --prompt "Fix the login redirect")
zoes session wait "$id"
zoes session read "$id"
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

## Put work on the Board

```sh
zoes ticket create --project api "Fix the login redirect"
zoes ticket create --start --title "Login redirect" - < notes.md
```

A ticket waits in To do until the user starts it. `--start` queues it instead, and it starts in its own worktree when the Board has a free slot. The prompt's first line is the title unless you pass `--title`. Without `--project` it takes the project holding the current folder. `ticket create` prints the ticket's id.

```sh
id=$(zoes ticket create "Fix the login redirect")
zoes ticket read "$id"
zoes ticket update --title "Login redirect" --prompt - "$id" < notes.md
zoes ticket delete "$id"
```

`update` changes only the flags you pass, and only until the ticket starts. `delete` refuses a ticket with a live session.

Use a ticket for work the user will pick up or follow on the Board. Use a session when you need the reply yourself.

## Rules

- Use `--new-worktree` when sessions run side by side, or when the user may be working in the main worktree.
- Leave `--access` out unless the user asks for a level. Pass `full` only when they ask for it by name.
- Leave `--start` out unless the user asks for the work to start.
- Don't delete a ticket unless the user asks.
- Permission prompts are the user's. Never try to answer one.
- Don't close a session you didn't create unless the user asks.
- If `zoes` says a process inside a Zoes terminal can only read sessions, you are running inside Zoes. Don't retry; tell the user.
