package factory

import (
	"path"
	"slices"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/team"
)

// NeverAllowed is whether command is one only the user may let through: it
// merges, reviews or opens a pull request, forces a push, pushes to base, or
// removes a tree outside the checkout. base is the Ticket's base branch; ""
// skips the push to it.
func NeverAllowed(command, base string) bool {
	command = team.Unwrapped(command)
	parts := strings.FieldsFunc(command, func(r rune) bool { return strings.ContainsRune(";|&\n", r) })
	return slices.ContainsFunc(parts, func(part string) bool { return never(words(part), base) })
}

// RefusedPrefix is the first prefix of an allowlist that is on the list.
func RefusedPrefix(allow []string) (string, bool) {
	for _, prefix := range allow {
		if NeverAllowed(prefix, "") {
			return prefix, true
		}
	}
	return "", false
}

// words splits one command at spaces and drops the quotes around a word.
func words(command string) []string {
	out := strings.Fields(command)
	for i, w := range out {
		out[i] = strings.Trim(w, `"'`)
	}
	return out
}

func never(w []string, base string) bool {
	// FOO=bar before the command sets its environment.
	for len(w) > 0 && strings.Contains(w[0], "=") {
		w = w[1:]
	}
	if len(w) == 0 {
		return false
	}
	args := w[1:]
	switch path.Base(w[0]) {
	case "gh":
		// gh's own flags may come first: gh -R owner/repo pr merge.
		i := slices.Index(args, "pr")
		return i >= 0 && i+1 < len(args) && slices.Contains([]string{"merge", "review", "create"}, args[i+1])
	case "git":
		command, rest := gitCommand(args)
		return command == "push" && pushes(rest, base)
	case "rm":
		return removesOutside(args)
	}
	return false
}

// gitCommand is git's command and what follows it, past git's own flags (git -C dir push).
func gitCommand(args []string) (string, []string) {
	for i := 0; i < len(args); i++ {
		switch a := args[i]; {
		case a == "-C", a == "-c":
			i++
		case strings.HasPrefix(a, "-"):
		default:
			return a, args[i+1:]
		}
	}
	return "", nil
}

// pushes is whether a git push forces, or sends to base.
func pushes(args []string, base string) bool {
	var plain []string
	for _, a := range args {
		switch {
		case strings.HasPrefix(a, "--force"), a == "--all", a == "--mirror":
			return true
		case strings.HasPrefix(a, "--"):
		case strings.HasPrefix(a, "-"):
			if strings.Contains(a, "f") {
				return true
			}
		default:
			plain = append(plain, a)
		}
	}
	// The first plain word is the remote. The rest are refspecs.
	for _, ref := range plain[min(1, len(plain)):] {
		if strings.HasPrefix(ref, "+") {
			return true
		}
		_, to, ok := strings.Cut(ref, ":")
		if !ok {
			to = ref
		}
		if base != "" && strings.TrimPrefix(to, "refs/heads/") == base {
			return true
		}
	}
	return false
}

// removesOutside is whether an rm is recursive and forced and names a path that can leave the checkout.
func removesOutside(args []string) bool {
	var recursive, force bool
	var paths []string
	for _, a := range args {
		switch {
		case a == "--recursive":
			recursive = true
		case a == "--force":
			force = true
		case strings.HasPrefix(a, "--"):
		case strings.HasPrefix(a, "-"):
			recursive = recursive || strings.ContainsAny(a, "rR")
			force = force || strings.Contains(a, "f")
		default:
			paths = append(paths, a)
		}
	}
	return recursive && force && slices.ContainsFunc(paths, func(p string) bool {
		return path.IsAbs(p) || strings.HasPrefix(p, "~") || strings.Contains(p, "$") || slices.Contains(strings.Split(p, "/"), "..")
	})
}

// Owner is the live Factory Ticket agentID works for, as its lead or as a worker of its lead's team.
func Owner(agentID string, tickets []proto.Ticket, teams []proto.Team) (proto.Ticket, bool) {
	lead := agentID
	if tm, _, ok := team.Find(teams, agentID); ok {
		lead = tm.Lead
	}
	for _, t := range tickets {
		if lead != "" && t.AgentID == lead && t.State == proto.TicketLive && t.Station != "" {
			return t, true
		}
	}
	return proto.Ticket{}, false
}

func (e *Engine) owner(agentID string) (proto.Ticket, bool) {
	tickets, _ := e.Board.Snapshot()
	teams, _ := e.Teams.Store.List()
	return Owner(agentID, tickets, teams)
}

// Runs is whether agentID is a Factory session: a live Ticket's lead or one of its workers.
func (e *Engine) Runs(agentID string) bool {
	if e == nil {
		return false
	}
	_, ok := e.owner(agentID)
	return ok
}

// Never is whether req is a Factory session's command that only the user may let through.
func (e *Engine) Never(req proto.PermissionRequest) bool {
	if e == nil || req.Detail.Command == "" {
		return false
	}
	t, ok := e.owner(req.AgentID)
	if !ok {
		return false
	}
	bases := []string{t.Base}
	if t.Base == "" {
		// The project's default branch isn't known here. These two names cover it.
		bases = []string{"main", "master"}
	}
	return slices.ContainsFunc(bases, func(base string) bool { return NeverAllowed(req.Detail.Command, base) })
}
