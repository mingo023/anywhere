package providers

var Fx = Spec{
	ID:        "fx",
	Name:      "fx",
	Transport: ACP,
	Efforts:   []string{},
	Plan:      true,
	Chat:      Chat{Interrupt: true, SendWhileWorking: true},
	Flags:     Flags{Model: "--model"},
	// fx is also a JSON viewer.
	Help:     []string{"acp"},
	NotAgent: []string{"acp", "login", "logout", "models", "status", "help", "-h", "--help", "-v", "--version"},
}
