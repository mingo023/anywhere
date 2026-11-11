package providers

var Omp = Spec{
	ID:        "omp",
	Name:      "omp",
	Transport: PiRPC,
	Efforts:   piEfforts,
	Plan:      true,
	NeverAsks: true,
	Chat:      Chat{Interrupt: true, Compact: true, SendWhileWorking: true, SubAgents: true},
	Flags:     Flags{Model: "--model", Plan: [2]string{"--tools", "read,grep,glob,lsp"}},
	Args:      [3]string{"", "--resume", ""},
	Help:      []string{"--mode", "rpc"},
	NotAgent:  []string{"install", "remove", "uninstall", "update", "list", "config", "auth", "mcp", "--list-models", "--export", "-h", "--help", "-v", "--version"},
}
