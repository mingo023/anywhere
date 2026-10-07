package state

import "strings"

// Parse reads the access, plan, model and effort that a claude or codex
// argv asks for: the inverse of launch.Argv. An argv with no access flags is
// settings; one whose access it can't read is ask.
func Parse(provider string, argv []string) Launch {
	var mode, sandbox, model, effort string
	var bypass, approveForMe bool
	values := map[string]*string{"--model": &model}
	switches := map[string]*bool{}
	if provider == "claude" {
		values["--permission-mode"], values["--effort"] = &mode, &effort
		switches["--dangerously-skip-permissions"] = &bypass
	} else {
		values["-m"], values["-s"], values["--sandbox"] = &model, &sandbox, &sandbox
		switches["--approve-for-me"], switches["--dangerously-bypass-approvals-and-sandbox"] = &approveForMe, &bypass
	}
	for i := 1; i < len(argv) && argv[i] != "--"; i++ {
		name, value, inline := strings.Cut(argv[i], "=")
		if p := values[name]; p != nil {
			if !inline && i+1 < len(argv) {
				i++
				value = argv[i]
			}
			*p = value
		} else if p := switches[argv[i]]; p != nil {
			*p = true
		}
	}
	l := Launch{Access: "ask", Model: model, Effort: effort}
	switch {
	case bypass:
		l.Access = "full"
	case provider == "claude" && mode == "":
		l.Access = "settings"
	case provider == "claude":
		l = Mode(l, mode)
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
