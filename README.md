<h1 align="center">Anywhere</h1>

<p align="center">
  Watch and drive Claude Code, Codex and other coding agents, from your Mac and your phone.
</p>

<p align="center">
  <a href="https://getanywhere.dev">Website</a> ·
  <a href="https://github.com/mingo023/anywhere/releases/latest/download/Anywhere.dmg">Download</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

![Anywhere](docs/screenshot.png)

Anywhere is a native macOS app for running many coding agents at once. Every `claude` and `codex` session runs in a real terminal, grouped by project and git worktree, and Anywhere tells you which ones are working, which need you and which are done.

## Features

- **Every agent, one window.** Real terminals in tabs and splits, grouped by project and worktree.
- **Know who needs you.** Status in the sidebar, inbox, Dock badge and notifications; jump straight to the session that's asking.
- **Review beside the terminal.** Browse files, read diffs and commit without leaving the session that made the change.
- **Worktrees from a branch or PR.** Create a worktree and start an agent in it, with your setup script run for you.
- **Automations.** Schedule agent runs, and get told when one needs a decision.
- **Scriptable.** Settings → General → Command line tool adds an `anywhere` command that starts, prompts and reads sessions, with a skill that teaches Claude Code and Codex to use it.
- **Sessions that survive.** A daemon owns the terminals, so agents keep running when you close the window.
- **On your phone, too.** Answer an agent from anywhere; your phone reaches your Mac over Tailscale.

## Install

Download [Anywhere.dmg](https://github.com/mingo023/anywhere/releases/latest/download/Anywhere.dmg) and drag Anywhere to Applications.

Requires macOS 13 or later. Updates install automatically through Sparkle.

On Linux (x86_64) or Windows (x64), unpack [Anywhere-linux-amd64.tar.gz](https://github.com/mingo023/anywhere/releases/latest/download/Anywhere-linux-amd64.tar.gz) or [Anywhere-windows-amd64.zip](https://github.com/mingo023/anywhere/releases/latest/download/Anywhere-windows-amd64.zip) and run `pocket-desktop`. These builds are unsigned and don't update themselves.

## Agents

Anywhere runs the agents it finds on your Mac. Install each one you want, then sign in to it:

- **Claude Code**: [install](https://claude.com/product/claude-code), then `claude auth login`
- **Codex**: [install](https://developers.openai.com/codex/cli), then `codex login`
- **Cursor**: `curl https://cursor.com/install -fsS | bash`, then `cursor-agent login`
- **Grok Build**: `curl -fsSL https://x.ai/cli/install.sh | bash`, then `grok login`
- **OpenCode**: `npm i -g opencode-ai`, then `opencode auth login`
- **Pi**: `npm i -g @earendil-works/pi-coding-agent`, then run `pi` and type `/login`
- **omp**: `curl -fsSL https://omp.sh/install | sh`, then run `omp` once to sign in
- **fx**: `curl -fsSL https://fx.sh/setup.sh | bash`, then `fx login`
- **Hermes Agent**: `curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash`, then `hermes model`
- **Antigravity**: `curl -fsSL https://antigravity.google/cli/install.sh | bash`, then run `agy` once to sign in

Claude Code and Codex run in a terminal or as a chat. The others run as a chat: in a terminal Anywhere lists them but can't tell when they need you. Pi and omp never ask before they use a tool, so they always run with Full access.

## Architecture

| Package | What it is |
|---|---|
| [`packages/desktop`](packages/desktop) | The desktop app, in Rust with [GPUI](https://www.gpui.rs) |
| [`packages/pocketd`](packages/pocketd) | The Go daemon that owns the terminals and outlives the app |
| [`packages/ios`](packages/ios) | The phone app, native SwiftUI ([ADR 0007](docs/adr/0007-native-ios-app.md)) |
| [`packages/protocol`](packages/protocol) | The protocol the daemon and its clients share |

Design decisions are recorded in [`docs/adr`](docs/adr), and the vocabulary used across the code in [`CONTEXT.md`](CONTEXT.md).

## Build from source

```sh
make app        # Anywhere Dev.app, side by side with the release app
make run        # release build of the desktop app, run in place
make ios        # build and test the phone app's Swift package
```

The phone app's targets need Xcode; [`packages/ios`](packages/ios/README.md) says how to build them.

`scripts/release-mac.sh <version>` builds the signed, notarized DMG and `scripts/release-desktop.sh <version> <linux-amd64|windows-amd64>` the Linux and Windows archives. Running the Release workflow from the Actions tab, or pushing a `v*` tag, publishes all three ([ADR 0004](docs/adr/0004-release-and-updates.md)).
