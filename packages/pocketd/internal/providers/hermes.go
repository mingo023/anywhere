package providers

// Hermes takes a model and a first prompt only as `hermes chat`, which Args and launch run.
// NotAgent is every other command its help lists.
var Hermes = Spec{
	ID:        "hermes",
	Name:      "Hermes Agent",
	Transport: ACP,
	Efforts:   []string{},
	Plan:      true,
	Chat:      Chat{Interrupt: true, SendWhileWorking: true, SubAgents: true},
	Flags:     Flags{Model: "-m", Effort: "--reasoning", Full: "--yolo"},
	Args:      [3]string{"-q", "chat --resume", ""},
	Sub:       "chat",
	SignIn:    []string{"model"},
	NotAgent: []string{"model", "moa", "fallback", "worktree", "browser", "secrets", "egress", "migrate", "codex-runtime", "gateway", "proxy", "lsp", "setup",
		"whatsapp", "whatsapp-cloud", "slack", "send", "logout", "auth", "status", "pause", "resume", "cron", "webhook", "peer", "portal", "kanban", "project",
		"hooks", "doctor", "verify", "security", "approvals", "dump", "debug", "backup", "checkpoints", "import", "config", "skin", "console", "pairing",
		"skills", "bundles", "plugins", "curator", "pets", "journey", "learning", "memory-graph", "memory", "tools", "computer-use", "mcp", "connectors",
		"sessions", "insights", "usage", "monitoring", "vault", "update", "uninstall", "acp", "profile", "completion", "dashboard", "serve", "desktop", "gui",
		"logs", "prompt-size", "-z", "--oneshot", "-h", "--help", "-V", "--version"},
}
