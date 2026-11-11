package providers

var Antigravity = Spec{
	ID:        "antigravity",
	Name:      "Antigravity",
	Command:   "agy",
	Transport: ACP,
	Efforts:   []string{},
	Plan:      true,
	Chat:      Chat{Interrupt: true, SendWhileWorking: true},
	Flags:     Flags{Model: "--model", Full: "--dangerously-skip-permissions", Plan: [2]string{"--mode", "plan"}},
	// agy signs in the first time it runs.
	SignInBare: true,
	NotAgent:   []string{"acp", "-h", "--help", "-v", "--version"},
}
