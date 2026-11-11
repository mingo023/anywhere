package providers

var piEfforts = []string{"off", "minimal", "low", "medium", "high", "xhigh", "max"}

var Pi = Spec{
	ID:        "pi",
	Name:      "Pi",
	Transport: PiRPC,
	Efforts:   piEfforts,
	Plan:      true,
	NeverAsks: true,
	Chat:      Chat{Interrupt: true, Compact: true, SendWhileWorking: true},
	Flags:     Flags{Model: "--model", Effort: "--thinking", Plan: [2]string{"--tools", "read,grep,find,ls"}},
	Args:      [3]string{"--", "--session", "--fork " + SessionID},
	Help:      []string{"--mode", "rpc"},
	NotAgent:  []string{"install", "remove", "uninstall", "update", "list", "config", "auth", "mcp", "--list-models", "--export", "-h", "--help", "-v", "--version"},
}
