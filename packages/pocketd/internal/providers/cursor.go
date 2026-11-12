package providers

var Cursor = Spec{
	ID:        "cursor",
	Name:      "Cursor",
	Command:   "cursor-agent",
	Transport: ACP,
	Efforts:   []string{},
	Plan:      true,
	Chat:      Chat{Interrupt: true, SendWhileWorking: true, SubAgents: true},
	Flags:     Flags{Model: "--model", Full: "--force", Plan: [2]string{"--mode", "plan"}},
	Args:      [3]string{"", "--resume", ""},
	SignIn:    []string{"login"},
	NotAgent: []string{"acp", "login", "logout", "status", "whoami", "about", "models", "mcp", "update", "upgrade", "create-chat", "generate-rule", "rule",
		"install-shell-integration", "uninstall-shell-integration", "help", "-h", "--help", "-v", "--version"},
}
