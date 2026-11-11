// Package providers lists the agent CLIs pocketd runs. It imports nothing
// internal, so proto, config and the rest can all read it.
package providers

// SessionID marks where fork args take the source session's id.
const SessionID = "{sessionId}"

// How pocketd's chat talks to a provider behind `pocketd pipe`.
const (
	StreamJSON = "stream-json"
	AppServer  = "app-server"
	ACP        = "acp"
	PiRPC      = "pi"
)

// Spec is one agent CLI pocketd runs. ID is its config.json section.
type Spec struct {
	ID        string
	Name      string    // as its maker writes it
	Command   string    // runs it, when not ID
	Transport string    // of its chat
	Efforts   []string  // a session may start with; never nil, sent as []
	Plan      bool      // may start planning first
	NeverAsks bool      // runs every tool without asking, so its access is always full
	Chat      Chat      // what its chat driver does
	Flags     Flags     // what its TUI takes, for one launch and state run from this Spec
	Args      [3]string // default promptArgs, resumeArgs, forkArgs (config.ArgKeys order)
	NotAgent  []string  // argv[1]s that run it as a tool, not a session
	Help      []string  // what its --help says, where another command has its name
}

// Flags name what a launch asks for on a provider's command line; "" where it has no flag for it.
type Flags struct {
	Model, Effort string
	Full          string    // stops it asking before it runs a tool
	Plan          [2]string // the flag and value that start it planning
	// Session runs the chat on a session, creating it if it was never written.
	Session string
}

// Cmd is the command that runs p in a terminal.
func (p *Spec) Cmd() string {
	if p.Command != "" {
		return p.Command
	}
	return p.ID
}

// OnlyChats says p tells its status only to its chat: its TUI has no hooks,
// so a session pocketd starts there with a prompt would never be seen to end.
func (p *Spec) OnlyChats() bool { return p.Transport == ACP || p.Transport == PiRPC }

// Chat is what a provider's chat driver does; clients hide the rest.
type Chat struct {
	Interrupt        bool `json:"interrupt"`
	Compact          bool `json:"compact"`
	SendWhileWorking bool `json:"sendWhileWorking"`
	SubAgents        bool `json:"subAgents"`
	// SendNow has the agent take what was sent mid-turn at once rather than after its current step.
	SendNow bool `json:"sendNow"`
}

// All is every provider, in the order clients list them.
var All = []*Spec{&Claude, &Codex, &Cursor, &Grok, &OpenCode, &Pi, &Omp, &Fx, &Hermes, &Antigravity}

// Get is the provider named id, or nil.
func Get(id string) *Spec {
	for _, p := range All {
		if p.ID == id {
			return p
		}
	}
	return nil
}

// NeverAsks says the provider named id runs every tool without asking.
func NeverAsks(id string) bool {
	p := Get(id)
	return p != nil && p.NeverAsks
}

// IDs are the providers' ids, in All's order.
func IDs() []string {
	var ids []string
	for _, p := range All {
		ids = append(ids, p.ID)
	}
	return ids
}
