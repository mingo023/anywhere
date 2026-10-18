package state

import (
	"strings"

	"pocketd/internal/providers"
)

var parsers = map[string]func([]string) Launch{providers.Claude.ID: parseClaude, providers.Codex.ID: parseCodex}

// Parse reads the access, plan, model and effort that a claude or codex
// argv asks for: the inverse of launch.Argv. An argv with no access flags is
// settings; one whose access it can't read is ask.
func Parse(provider string, argv []string) Launch {
	if parse := parsers[provider]; parse != nil {
		return parse(argv)
	}
	return codexLaunch(argv, false)
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
