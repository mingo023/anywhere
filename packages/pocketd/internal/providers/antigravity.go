package providers

var Antigravity = Spec{
	ID:          "antigravity",
	Name:        "Antigravity",
	Command:     "agy",
	ChatCommand: "agy_acp_server.par",
	Transport:   ACP,
	Efforts:     []string{},
	Chat:        Chat{Interrupt: true, SendWhileWorking: true},
	NotAgent:    []string{"-h", "--help", "-v", "--version"},
}
