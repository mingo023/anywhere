package launch

import (
	"regexp"
	"strings"
	"unicode"

	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

var resumeID = regexp.MustCompile(`^[A-Za-z0-9._:-]{1,128}$`)

// Resume is the argv that brings t's Conversation back with the access it
// had. Full comes back as ask, so a restart never re-arms an unattended
// bypass; state.Outcome reports that.
func Resume(t state.Terminal) ([]string, *Failure) {
	l := state.Launch{Access: "ask"}
	if t.Launch != nil {
		l = *t.Launch
	}
	if !resumeID.MatchString(t.ConversationID) {
		return nil, invalidResume("conversation id")
	}
	var argv []string
	switch t.Provider {
	case "claude":
		mode := map[string]string{"edits": "acceptEdits", "auto": "auto"}[l.Access]
		if l.Plan {
			mode = "plan"
		}
		if mode == "" && l.Access != "settings" {
			mode = "default"
		}
		argv = []string{"claude", "--resume", t.ConversationID}
		if mode != "" {
			argv = append(argv, "--permission-mode", mode)
		}
		if l.Model != "" {
			argv = append(argv, "--model", l.Model)
		}
		if l.Effort != "" {
			argv = append(argv, "--effort", l.Effort)
		}
	case "codex":
		argv = []string{"codex", "resume", t.ConversationID}
		switch l.Access {
		case "settings":
		case "edits":
			argv = append(argv, "-s", "workspace-write", "-a", "on-request")
		case "auto":
			argv = append(argv, "--approve-for-me")
		default:
			argv = append(argv, "-s", "read-only", "-a", "on-request")
		}
		if l.Model != "" {
			argv = append(argv, "-m", l.Model)
		}
	default:
		return nil, invalidResume("provider " + t.Provider)
	}
	return argv, ValidResume(argv)
}

// ValidResume refuses a resume argv a corrupt or hand-edited state file could
// have bent into more than a plain command on PATH with ordinary arguments.
func ValidResume(argv []string) *Failure {
	total := 0
	for _, a := range argv {
		total += len(a)
		if strings.ContainsRune(a, '\'') || strings.ContainsFunc(a, unicode.IsControl) {
			return invalidResume("quote or control character")
		}
	}
	switch {
	case len(argv) == 0 || argv[0] == "" || strings.Contains(argv[0], "/"):
		return invalidResume("command")
	case len(argv) > 64 || total > 8<<10:
		return invalidResume("too long")
	}
	return nil
}

func invalidResume(detail string) *Failure {
	return &Failure{Code: "invalid_resume_argv", Message: "Pocket can't resume this session", Detail: detail}
}

// resumeCmd is how Daemon.Resume runs saved's resume argv in a login shell,
// or the reason it can't.
func resumeCmd(saved state.Terminal, shell, exe string, env []string) (string, []string, string) {
	argv, f := Resume(saved)
	if f != nil {
		return "", nil, f.Code
	}
	if _, err := terminal.LookPath(argv[0], env); err != nil {
		return "", nil, "resume_not_accepted"
	}
	return shell, Wrap(shell, exe, "", argv), ""
}
