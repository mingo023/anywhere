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
// skips the push to it. A command that might do one of these counts as doing it:
// a false alarm costs an Ask, a miss breaks "no agent merges".
func NeverAllowed(command, base string) bool {
	command = unquote.Replace(command)
	parts := strings.FieldsFunc(command, func(r rune) bool { return strings.ContainsRune(";|&\n()`", r) })
	return slices.ContainsFunc(parts, func(part string) bool { return never(strings.Fields(part), base) })
}

// unquote drops the quotes and backslashes with which the shell builds a word out of pieces.
var unquote = strings.NewReplacer(`"`, "", `'`, "", `\`, "")

// RefusedPrefix is the first prefix of an allowlist that is on the list.
func RefusedPrefix(allow []string) (string, bool) {
	for _, prefix := range allow {
		if NeverAllowed(prefix, "") {
			return prefix, true
		}
	}
	return "", false
}

// runners hand the rest of their command line to a command that they start: the shells run it from -c, and
// the shell's keywords stand before the command of their branch.
var runners = []string{
	"env", "sudo", "doas", "command", "exec", "builtin", "time", "nohup", "nice", "ionice", "timeout", "xargs", "watch", "stdbuf", "setsid", "eval",
	"sh", "bash", "zsh", "dash", "ksh", "if", "then", "else", "elif", "while", "until", "do", "!", "{", "}",
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
	name := path.Base(w[0])
	switch {
	case slices.Contains(runners, name):
		// Its own options and arguments come first, so the command may start at any later word.
		for i := range args {
			if never(args[i:], base) {
				return true
			}
		}
		return false
	case name == "gh", strings.ContainsAny(w[0], "${"):
		return ghNever(args, base)
	case name == "git":
		command, rest := gitCommand(args)
		return command == "push" && pushes(rest, base)
	case name == "rm":
		return removesOutside(args)
	}
	return false
}

// prVerbs are what gh does to a pull request that only you may do.
var prVerbs = []string{"merge", "review", "create"}

// ghNever is whether gh with args merges, reviews or opens a pull request, or does so through its api command.
// gh's own flags may stand between the words: gh pr -R owner/repo merge.
func ghNever(args []string, base string) bool {
	args = strings.FieldsFunc(strings.Join(args, " "), func(r rune) bool { return strings.ContainsRune(" {},", r) })
	if i := slices.Index(args, "pr"); i >= 0 && slices.ContainsFunc(args[i+1:], func(a string) bool { return slices.Contains(prVerbs, a) }) {
		return true
	}
	return slices.Contains(args, "api") && apiNever(args, base)
}

// apiNever is whether a gh api call's words reach the merge, the reviews or the creation of a pull request,
// or the base branch's ref. A GET of those counts too.
func apiNever(args []string, base string) bool {
	var writes, creates bool
	for _, a := range args {
		switch strings.ToLower(a) {
		case "-f", "--field", "--raw-field", "--input", "post":
			writes = true
		}
	}
	for _, a := range args {
		if lower := strings.ToLower(a); slices.ContainsFunc([]string{"mergepullrequest", "pullrequestreview", "createpullrequest", "automerge", "updateref", "createref"}, func(m string) bool { return strings.Contains(lower, m) }) {
			return true
		}
		route, _, _ := strings.Cut(a, "?")
		segments := strings.Split(strings.Trim(route, "/"), "/")
		if slices.ContainsFunc(segments, func(s string) bool { return s == "merge" || s == "merges" || s == "reviews" }) {
			return true
		}
		creates = creates || segments[len(segments)-1] == "pulls" && writes
		if base != "" && strings.HasSuffix(strings.Join(segments, "/"), "git/refs/heads/"+base) {
			return true
		}
	}
	return creates
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
