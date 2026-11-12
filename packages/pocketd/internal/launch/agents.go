package launch

import (
	"errors"
	"math/rand/v2"
	"net"
	"slices"
	"strconv"
	"strings"

	"pocketd/internal/config"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/state"
)

// specCLI runs a provider from its providers.Spec: its TUI by Spec.Flags, and
// its chat by Spec.Transport.
type specCLI struct{ p *providers.Spec }

// acpArgv adds what runs argv's command as an ACP server on stdio. It has
// flags only for what the CLI takes as one; the rest of l is carried.
func acpArgv(p *providers.Spec, l state.Launch, argv []string) []string {
	switch p {
	// --no-leader is as MonoCode starts Grok's server; Grok doesn't say what
	// it does without it.
	case &providers.Grok:
		argv = append(argv, "--no-auto-update")
		argv = append(flags(p, l, append(plan(p, l, argv), "agent", "--no-leader")), "stdio")
	case &providers.Fx:
		argv = flags(p, l, append(argv, "acp"))
	// OpenCode's ACP server runs on its HTTP server, the one place that names
	// a sub-agent's session while it runs.
	case &providers.OpenCode:
		argv = append(argv, "acp", "--port", strconv.Itoa(freePort()))
	default:
		argv = append(argv, "acp")
	}
	return argv
}

// freePort is a port free on loopback, or 0, which leaves the pick to the
// agent. It lies below the systems' ephemeral ranges, so no socket is handed
// it before the agent binds it: OpenCode exits on a port taken.
func freePort() int {
	for range 10 {
		port := 20000 + rand.IntN(12000)
		if l, err := net.Listen("tcp", "127.0.0.1:"+strconv.Itoa(port)); err == nil {
			l.Close()
			return port
		}
	}
	return 0
}

// piArgv adds what runs argv's command for pi's RPC on stdio. Planning is
// pi's read-only tools.
func piArgv(p *providers.Spec, l state.Launch, argv []string) []string {
	return flags(p, l, plan(p, l, append(argv, "--mode", "rpc")))
}

// plan adds what starts p planning, where it has a flag for that.
func plan(p *providers.Spec, l state.Launch, argv []string) []string {
	if l.Plan && p.Flags.Plan[0] != "" {
		argv = append(argv, p.Flags.Plan[:]...)
	}
	return argv
}

// flags adds l's model, effort and full access where p has a flag for them.
func flags(p *providers.Spec, l state.Launch, argv []string) []string {
	if l.Model != "" && p.Flags.Model != "" {
		argv = append(argv, p.Flags.Model, l.Model)
	}
	if l.Effort != "" && p.Flags.Effort != "" {
		argv = append(argv, p.Flags.Effort, l.Effort)
	}
	if l.Access == "full" && !l.Plan && p.Flags.Full != "" {
		argv = append(argv, p.Flags.Full)
	}
	return argv
}

// chatArgv is argv run for the chat, carrying l for the driver to read back
// with state.Parse: an ACP agent takes most of it over the protocol.
func (c specCLI) chatArgv(l state.Launch, argv []string) []string {
	if c.p.Transport == providers.PiRPC {
		argv = piArgv(c.p, l, argv)
	} else {
		argv = acpArgv(c.p, l, argv)
	}
	return append(argv, l.Carried()...)
}

func (c specCLI) validEffort(effort string) string {
	switch {
	case effort == "":
	case len(c.p.Efforts) == 0 && !model.MatchString(effort):
		return badEffort
	case len(c.p.Efforts) > 0 && !slices.Contains(c.p.Efforts, effort):
		return c.p.Name + "'s effort is one of " + strings.Join(c.p.Efforts, ", ")
	}
	return ""
}

func (c specCLI) switchEffort(effort string) error {
	if msg := c.validEffort(effort); msg != "" {
		return errors.New(msg)
	}
	return nil
}

// chat leaves an ACP fork in the TUI: only pi forks from the command line.
func (c specCLI) chat(s proto.LaunchSpec) bool {
	return s.Chat && (s.Fork == "" || c.p.Transport == providers.PiRPC)
}

func (c specCLI) attaches(s proto.LaunchSpec) bool { return c.chat(s) }

func (c specCLI) argv(s proto.LaunchSpec, argv []string, _, _ string) ([]string, bool, *Failure) {
	if msg := c.validEffort(s.Effort); msg != "" {
		return nil, false, fail("invalid_spec", msg)
	}
	if s.Plan && !c.chat(s) && c.p.Flags.Plan[0] == "" {
		return nil, false, fail("access_not_allowed", c.p.Name+" can't plan first in a terminal session")
	}
	l := state.Launch{Access: Access(s), Plan: s.Plan, Model: s.Model, Effort: s.Effort, Options: s.Options}
	if c.chat(s) {
		return c.chatArgv(l, argv), true, nil
	}
	if c.p.Sub != "" {
		argv = append(argv, c.p.Sub)
	}
	return flags(c.p, l, plan(c.p, l, argv)), false, nil
}

// chatResume runs the chat again. pi takes the session on its command line;
// an ACP agent's is loaded over the protocol. pi writes no session before its
// first prompt, so it is told to create the one it may not find.
func (c specCLI) chatResume(t state.Terminal, l state.Launch) ([]string, bool) {
	if !t.Chat {
		return nil, false
	}
	argv := []string{c.p.Cmd()}
	if c.p.Transport == providers.PiRPC {
		resume := config.DefaultArgs(c.p.ID).Resume
		if c.p.Flags.Session != "" {
			resume = []string{c.p.Flags.Session}
		}
		argv = append(append(argv, resume...), t.ConversationID)
	}
	return c.chatArgv(c.resumed(t, l), argv), true
}

// resumed is l as a resume starts it: full comes back as ask, unless the
// agent has no other access.
func (c specCLI) resumed(t state.Terminal, l state.Launch) state.Launch {
	if l.Access == "full" && !t.KeepAccess && !c.p.NeverAsks {
		l.Access = "ask"
	}
	return l
}

func (c specCLI) resume(t state.Terminal, l state.Launch, argv []string) []string {
	l = c.resumed(t, l)
	return flags(c.p, l, plan(c.p, l, argv))
}

func (specCLI) untrusted([]string, string) *Failure { return nil }

func (c specCLI) login(exe string) []string {
	if c.p.SignIn == nil && !c.p.SignInBare {
		return nil
	}
	return append([]string{exe}, c.p.SignIn...)
}

// logout is none: pocketd keeps no accounts for these.
func (specCLI) logout(string) []string { return nil }
