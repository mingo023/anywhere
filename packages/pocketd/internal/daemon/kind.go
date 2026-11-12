package daemon

import (
	"errors"

	"pocketd/internal/agent"
	"pocketd/internal/providers"
	"pocketd/internal/state"
)

// kind is what a provider's process needs from the daemon beyond its Driver.
type kind interface {
	tool(arg string) bool                  // argv[1] runs a helper, not the agent
	serves(arg string) bool                // argv[1] runs it for the chat behind a pipe
	pipe(d *Daemon, pr *presence) pipeChat // its chat behind the pipe pr runs in
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

// kindOf has every providers.All id, the only ones Provider returns, by the transport of its chat.
var kindOf = func() map[string]kind {
	byTransport := map[string]kind{providers.StreamJSON: claudeKind{}, providers.AppServer: codexKind{}, providers.ACP: acpKind{}, providers.PiRPC: piKind{}}
	m := map[string]kind{}
	for _, p := range providers.All {
		m[p.ID] = byTransport[p.Transport]
	}
	return m
}()

// tuiKind is an agent whose TUI tells pocketd nothing: its Session shows
// only what its Terminal does, and is driven from its chat or not at all.
type tuiKind struct{}

func (tuiKind) tool(string) bool { return false }

func (tuiKind) started(*Daemon, *presence) {}

func (tuiKind) attach(_ *Daemon, pr *presence) { pr.a.SetAttached(false) }

func (tuiKind) typeIn(c termDriver, text string) error { return c.t.Prompt(text) }

var errTUISettings = errors.New("Switch this agent's model in its terminal")

func (tuiKind) settings(termDriver, string, string) error { return errTUISettings }

func (tuiKind) keys(*Daemon, *presence, []byte) {}

func (tuiKind) resumable(state.Terminal) bool { return true }

// acpKind is an agent whose chat is an ACP server on stdio. Its pipe is newACPChat, in acp_chat.go.
type acpKind struct{ tuiKind }

// serves the subcommand most run their ACP server as. Grok's comes after
// flags, which detect.go reads without it.
func (acpKind) serves(arg string) bool { return arg == "acp" }

func (acpKind) pipe(d *Daemon, pr *presence) pipeChat { return newACPChat(d, pr) }

// piKind is pi or omp, whose chat is `--mode rpc` on stdio. Its pipe is newPiChat, in pi_chat.go.
type piKind struct{ tuiKind }

func (piKind) serves(string) bool { return false }

func (piKind) pipe(d *Daemon, pr *presence) pipeChat { return newPiChat(d, pr) }
