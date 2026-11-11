package state

import (
	"slices"
	"strings"

	"pocketd/internal/providers"
)

// Carry ends a chat argv's own words. What follows is the launch's access,
// plan, model and effort, for an agent whose chat takes them over its
// protocol rather than as flags: `pocketd pipe` runs only what comes before
// it, and Parse reads what comes after.
const Carry = "--pocketd"

// Carried is l as the words a chat argv ends with.
func (l Launch) Carried() []string {
	argv := []string{Carry, "--access", l.Access}
	if l.Plan {
		argv = append(argv, "--plan")
	}
	if l.Model != "" {
		argv = append(argv, "--model", l.Model)
	}
	if l.Effort != "" {
		argv = append(argv, "--effort", l.Effort)
	}
	return argv
}

// Cut splits a chat argv at Carry: the command to run, and the Launch it
// carries. ok is false for an argv that carries none.
func Cut(argv []string) (run []string, l Launch, ok bool) {
	i := slices.Index(argv, Carry)
	if i < 0 {
		return argv, Launch{}, false
	}
	var access string
	scan(argv[i:], map[string]*string{"--access": &access, "--model": &l.Model, "--effort": &l.Effort}, map[string]*bool{"--plan": &l.Plan}, nil)
	if l.Access = "ask"; slices.Contains([]string{"settings", "edits", "auto", "full"}, access) {
		l.Access = access
	}
	return argv[:i], l, true
}

// parseFlags reads the TUI argv of a provider run from its Spec's Flags. It
// stops at the flag that takes the prompt, as scan does at "--".
func parseFlags(p *providers.Spec) func([]string) Launch {
	return func(argv []string) Launch {
		if words := strings.Fields(p.Args[0]); len(words) > 0 {
			if i := slices.Index(argv, words[len(words)-1]); i > 0 {
				argv = argv[:i]
			}
		}
		var plan string
		var full bool
		l := Launch{Access: "settings"}
		values, switches := map[string]*string{}, map[string]*bool{}
		for flag, v := range map[string]*string{p.Flags.Model: &l.Model, p.Flags.Effort: &l.Effort, p.Flags.Plan[0]: &plan} {
			if flag != "" {
				values[flag] = v
			}
		}
		if p.Flags.Full != "" {
			switches[p.Flags.Full] = &full
		}
		scan(argv, values, switches, nil)
		if full {
			l.Access = "full"
		}
		l.Plan = plan != "" && plan == p.Flags.Plan[1]
		return l
	}
}
