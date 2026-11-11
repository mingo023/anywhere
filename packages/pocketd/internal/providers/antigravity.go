package providers

var Antigravity = Spec{
	ID:        "antigravity",
	Name:      "Antigravity",
	Command:   "agy",
	Transport: ACP,
	Efforts:   []string{},
	Chat:      Chat{Interrupt: true, SendWhileWorking: true},
	NotAgent:  []string{"acp", "-h", "--help", "-v", "--version"},
}
