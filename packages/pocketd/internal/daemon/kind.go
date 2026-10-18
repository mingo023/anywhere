package daemon

import (
	"pocketd/internal/agent"
	"pocketd/internal/providers"
	"pocketd/internal/state"
)

// kind is what a provider's process needs from the daemon beyond its Driver.
type kind interface {
	tool(arg string) bool   // argv[1] runs a helper, not the agent
	serves(arg string) bool // argv[1] runs it for the chat behind a pipe
	pipe(d *Daemon, pr *presence) pipeChat
	started(d *Daemon, pr *presence)
	attach(d *Daemon, pr *presence)
	typeIn(c termDriver, text string) error
	settings(c termDriver, model, effort string) error
	keys(d *Daemon, pr *presence, b []byte)
	resumable(e state.Terminal) bool
}

// pipeChat is a provider's chat behind `pocketd pipe`, in place of its TUI.
type pipeChat interface {
	driver(c termDriver) agent.Driver
	attach()
	out(line string) // a line the agent wrote
}

// kindOf has every providers.All id, the only ones Provider returns.
var kindOf = map[string]kind{providers.Claude.ID: claudeKind{}, providers.Codex.ID: codexKind{}}
