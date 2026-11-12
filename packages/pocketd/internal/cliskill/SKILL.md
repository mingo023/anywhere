---
name: anywhere
description: Run and follow coding agent sessions (Claude Code, Codex and the other agents it runs) in Anywhere, the user's desktop app for coding agents, with the `anywhere` command. Use when the user asks to start a session in Anywhere, manage tickets on its Board, hand work to another agent there, send a session a prompt, check on sessions, or read what one replied.
---

# anywhere

`anywhere` acts as the user on this Mac. Every session it starts shows in the Anywhere app like one the user opened.

## Commands

```sh
anywhere project list [--json]
anywhere session list [--json]
anywhere session create [--project <name|path>] [--worktree <name> | --new-worktree [<name>] [--base <branch>]] [--provider <agent>] [--access settings|ask|edits|auto|full] [--prompt <text>]
anywhere session prompt <id> <text|->
anywhere session wait [--timeout 30m] <id>
anywhere session read [--limit 200] [--json] <id>
anywhere session close <id>
anywhere ticket create [--project <name|path>] [--provider <agent>] [--base <branch>] [--title <text>] [--start] <prompt|->
anywhere ticket list [--json]
anywhere ticket read [--json] <id>
anywhere ticket update [--title <text>] [--provider <agent>] [--base <branch>] [--prompt <text|->] <id>
anywhere ticket delete <id>
```

`<agent>` is `claude` when left out; `anywhere` run with no arguments lists the others it can start. `create` prints the session's id. Without `--project` it takes the project holding the current folder. Without a worktree flag it runs in the project's main worktree, the same folder the user works in. `--new-worktree` makes a worktree from the default branch (`main`), or from `--base`; without a name, Anywhere names it from the prompt, as the app does. `prompt <id> -` reads the text from stdin.

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

## Put work on the Board

```sh
anywhere ticket create --project api "Fix the login redirect"
anywhere ticket create --start --title "Login redirect" - < notes.md
```

A ticket waits in To do until the user starts it. `--start` queues it instead, and it starts in its own worktree when the Board has a free slot. The prompt's first line is the title unless you pass `--title`. Without `--project` it takes the project holding the current folder. `ticket create` prints the ticket's id.

```sh
id=$(anywhere ticket create "Fix the login redirect")
anywhere ticket read "$id"
anywhere ticket update --title "Login redirect" --prompt - "$id" < notes.md
anywhere ticket delete "$id"
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
- If `anywhere` says a process inside an Anywhere terminal can only read sessions, you are running inside Anywhere. Don't retry; tell the user.
