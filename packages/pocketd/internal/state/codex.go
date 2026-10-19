package state

func parseCodex(argv []string) Launch { return codexLaunch(argv, true) }

// codexLaunch reads codex's TUI flags, and with config its app-server's -c keys too.
func codexLaunch(argv []string, config bool) Launch {
	var sandbox, model, effort string
	var bypass, approveForMe bool
	var keys func(key, value string)
	if config {
		keys = func(key, value string) {
			switch key {
			case "sandbox_mode":
				sandbox = value
			case "model":
				model = value
			case "model_reasoning_effort":
				effort = value
			case "approvals_reviewer":
				approveForMe = value == "auto_review"
			}
		}
	}
	scan(argv,
		map[string]*string{"-m": &model, "--model": &model, "-s": &sandbox, "--sandbox": &sandbox},
		map[string]*bool{"--approve-for-me": &approveForMe, "--dangerously-bypass-approvals-and-sandbox": &bypass},
		keys)
	l := Launch{Access: "ask", Model: model, Effort: effort}
	switch {
	case bypass:
		l.Access = "full"
	case approveForMe:
		l.Access = "auto"
	case sandbox == "":
		l.Access = "settings"
	case sandbox == "danger-full-access":
		l.Access = "full"
	case sandbox == "workspace-write":
		l.Access = "edits"
	}
	return l
}
