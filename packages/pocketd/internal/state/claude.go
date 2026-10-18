package state

func parseClaude(argv []string) Launch {
	var mode, model, effort string
	var bypass bool
	scan(argv,
		map[string]*string{"--model": &model, "--permission-mode": &mode, "--effort": &effort},
		map[string]*bool{"--dangerously-skip-permissions": &bypass},
		nil)
	l := Launch{Access: "ask", Model: model, Effort: effort}
	switch {
	case bypass:
		l.Access = "full"
	case mode == "":
		l.Access = "settings"
	default:
		l = Mode(l, mode)
	}
	return l
}

// Mode applies a claude permission_mode; one it doesn't know leaves l as it was.
func Mode(l Launch, permissionMode string) Launch {
	access, ok := map[string]string{"default": "ask", "acceptEdits": "edits", "auto": "auto", "bypassPermissions": "full"}[permissionMode]
	switch {
	case permissionMode == "plan":
		l.Plan = true
	case ok:
		l.Access, l.Plan = access, false
	}
	return l
}
