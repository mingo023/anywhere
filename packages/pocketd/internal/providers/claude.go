package providers

var Claude = Spec{
	ID:       "claude",
	Efforts:  []string{"low", "medium", "high", "xhigh", "max"},
	Plan:     true,
	Chat:     Chat{Interrupt: true, Compact: true, SendWhileWorking: true, SubAgents: true},
	Args:     [3]string{"--", "--resume", "--resume {sessionId} --fork-session"},
	NotAgent: []string{"auth", "mcp", "doctor", "config", "update", "install", "plugin", "setup-token", "migrate-installer", "-v", "--version", "-h", "--help"},
}
