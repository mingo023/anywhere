package providers

var Codex = Spec{
	ID:       "codex",
	Efforts:  []string{},
	Plan:     false,
	Chat:     Chat{Interrupt: true, Compact: true, SendWhileWorking: true, SubAgents: false},
	Args:     [3]string{"--", "resume", "fork " + SessionID},
	NotAgent: []string{"login", "logout", "mcp", "mcp-server", "app-server", "completion", "sandbox", "debug", "apply", "a", "cloud", "features", "help", "-h", "--help", "-V", "--version"},
}
