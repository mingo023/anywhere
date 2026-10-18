// Package providers lists the agent CLIs pocketd runs. It imports nothing
// internal, so proto, config and the rest can all read it.
package providers

// Spec is one agent CLI pocketd runs. ID is its command's name and its config.json section.
type Spec struct {
	ID       string
	Efforts  []string  // a session may start with; never nil, sent as []
	Plan     bool      // may start planning first
	Chat     Chat      // what its chat driver does
	Args     [3]string // default promptArgs, resumeArgs, forkArgs (config.ArgKeys order)
	NotAgent []string  // argv[1]s that run it as a tool, not a session
}

// Chat is what a provider's chat driver does; clients hide the rest.
type Chat struct {
	Interrupt        bool `json:"interrupt"`
	Compact          bool `json:"compact"`
	SendWhileWorking bool `json:"sendWhileWorking"`
	SubAgents        bool `json:"subAgents"`
}

// All is every provider, in the order clients list them.
var All = []*Spec{&Claude, &Codex}

// Get is the provider named id, or nil.
func Get(id string) *Spec {
	for _, p := range All {
		if p.ID == id {
			return p
		}
	}
	return nil
}
