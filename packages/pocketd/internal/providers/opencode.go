package providers

var OpenCode = Spec{
	ID:        "opencode",
	Name:      "OpenCode",
	Transport: ACP,
	Efforts:   []string{},
	Plan:      true,
	Chat:      Chat{Interrupt: true, SendWhileWorking: true},
	Flags:     Flags{Model: "-m", Full: "--auto", Plan: [2]string{"--agent", "plan"}},
	Args:      [3]string{"--prompt", "-s", "-s " + SessionID + " --fork"},
	NotAgent: []string{"completion", "acp", "mcp", "attach", "run", "debug", "providers", "auth", "agent", "upgrade", "uninstall", "serve", "web", "models", "stats",
		"export", "import", "github", "session", "plugin", "plug", "db", "-h", "--help", "-v", "--version"},
}
