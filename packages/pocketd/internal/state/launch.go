package state

import (
	"strings"

	"pocketd/internal/providers"
)

var parsers = func() map[string]func([]string) Launch {
	m := map[string]func([]string) Launch{providers.Claude.ID: parseClaude, providers.Codex.ID: parseCodex}
	for _, p := range providers.All {
		if m[p.ID] == nil {
			m[p.ID] = parseFlags(p)
		}
	}
	return m
}()

// Parse reads the access, plan, model and effort that an agent's argv asks
// for: the inverse of launch.Argv. An argv with no access flags is settings;
// one whose access it can't read is ask. An agent that never asks is full
// whatever its argv says.
func Parse(provider string, argv []string) Launch {
	_, l, carried := Cut(argv)
	switch parse := parsers[provider]; {
	case carried:
	case parse != nil:
		l = parse(argv)
	default:
		l = codexLaunch(argv, false)
	}
	if providers.NeverAsks(provider) {
		l.Access = "full"
	}
	return l
}

// scan reads argv's flags up to "--" into values and switches, the last one
// winning. config, if set, gets each `-c key=value` with its quotes trimmed.
func scan(argv []string, values map[string]*string, switches map[string]*bool, config func(key, value string)) {
	for i := 1; i < len(argv) && argv[i] != "--"; i++ {
		name, value, inline := strings.Cut(argv[i], "=")
		if p := values[name]; p != nil {
			if !inline && i+1 < len(argv) {
				i++
				value = argv[i]
			}
			*p = value
		} else if p := switches[argv[i]]; p != nil {
			*p = true
		} else if config != nil && argv[i] == "-c" && i+1 < len(argv) {
			i++
			key, value, _ := strings.Cut(argv[i], "=")
			config(key, strings.Trim(value, `"`))
		}
	}
}
