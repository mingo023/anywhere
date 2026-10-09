package ops

import (
	"fmt"
	"maps"
	"slices"
	"strings"
	"time"
)

// Status is what pocketd status reports about the running host.
type Status struct {
	PID          int            `json:"pid"`
	Version      string         `json:"version"`
	Home         string         `json:"home"`
	Sock         string         `json:"sock"`
	Log          string         `json:"log"`
	Uptime       int64          `json:"uptime"` // seconds
	Listen       []string       `json:"listen"`
	Terminals    int            `json:"terminals"`
	Agents       map[string]int `json:"agents"` // by status
	KeepingAwake bool           `json:"keepingAwake"`
	Tailnet      bool           `json:"tailnet"`
	ShellEnv     string         `json:"shellEnv"`
	Service      string         `json:"service"` // "loaded", "installed" or "none"
	// Config is every config-set key with its value; a desktop writes only once it has seen it.
	Config map[string]any `json:"config,omitempty"`
	// Providers is each built-in agent's path on the login PATH, empty when it isn't found.
	Providers map[string]string `json:"providers,omitempty"`
}

func (s Status) Text() string {
	agents := "none"
	if len(s.Agents) > 0 {
		var parts []string
		for _, k := range slices.Sorted(maps.Keys(s.Agents)) {
			parts = append(parts, fmt.Sprintf("%d %s", s.Agents[k], k))
		}
		agents = strings.Join(parts, ", ")
	}
	yes := map[bool]string{true: "yes", false: "no"}
	listen := "none"
	if len(s.Listen) > 0 {
		listen = strings.Join(s.Listen, ", ")
	}
	return fmt.Sprintf(`pocketd %s, pid %d, up %s
home       %s
socket     %s
log        %s
listening  %s
service    %s
terminals  %d
agents     %s
awake      %s
tailnet    %s
shell env  %s
`, s.Version, s.PID, time.Duration(s.Uptime)*time.Second, s.Home, s.Sock, s.Log, listen, s.Service,
		s.Terminals, agents, yes[s.KeepingAwake], yes[s.Tailnet], s.ShellEnv)
}
