package providers

var Grok = Spec{
	ID:        "grok",
	Name:      "Grok Build",
	Transport: ACP,
	Efforts:   []string{"low", "medium", "high", "xhigh"},
	Plan:      true,
	Chat:      Chat{Interrupt: true, Compact: true, SendWhileWorking: true},
	Flags:     Flags{Model: "--model", Effort: "--reasoning-effort", Full: "--always-approve", Plan: [2]string{"--permission-mode", "plan"}},
	NotAgent:  []string{"agent", "login", "logout", "models", "mcp", "update", "help", "-h", "--help", "-V", "--version"},
}
