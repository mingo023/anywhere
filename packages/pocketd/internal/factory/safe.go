package factory

import (
	"regexp"
	"slices"
	"strings"
)

// safeCommands are the only gh and git calls that may pass without the user. The key is the tool and its
// subcommand; the rule, when there is one, reads the rest of the words and says whether they are safe. A word
// the table does not name is refused: new gh and git subcommands start out as an Ask.
//
// git merge, rebase, pull, reset, cherry-pick, tag, remote, config, submodule, bisect and worktree are left
// out: each moves a branch or runs another command. Workers get their branch and worktree from the engine, so
// they don't need them.
var safeCommands = map[string]func(args []string, base string) bool{
	"git status": nil, "git diff": nil, "git log": nil, "git show": nil, "git rev-parse": nil, "git ls-files": nil,
	"git add": nil, "git commit": nil, "git switch": nil, "git checkout": nil, "git restore": nil, "git fetch": nil,
	"git stash": nil, "git merge-base": nil,
	"git branch": branchKeeps,
	"git push":   pushesOwnBranch,

	"gh pr view": nil, "gh pr list": nil, "gh pr checks": nil, "gh pr diff": nil, "gh pr status": nil,
	"gh run view": nil, "gh run list": nil, "gh run watch": nil,
	"gh issue view": nil, "gh issue list": nil,
	"gh repo view": nil,
	"gh api":       apiGet,
}

// shellSafe is every character a safe call may hold. What the shell expands, splits, quotes or redirects on
// ($ { } ! \ ` quotes ; | & < > ( ) * ? [ ] # @ and the newline) is outside it.
var shellSafe = regexp.MustCompile(`^[A-Za-z0-9 \t_./:=,+%^~-]*$`)

// ghOrGit finds the word gh or git. On macOS the file system ignores case, so GH runs gh.
var ghOrGit = regexp.MustCompile(`(?i)(?:^|[^a-z0-9_])(?:gh|git)(?:$|[^a-z0-9_])`)

// expansion is what the shell may drop from a word: ${x}, $(x), $x, `x`, and the quotes and backslashes.
var expansion = regexp.MustCompile("\\$\\{[^}]*\\}|\\$\\([^)]*\\)|\\$[A-Za-z_0-9@*#?$!-]*|`[^`]*`|[\"'\\\\]")

// message is the quoted value of git commit -m. Single quotes hold anything. Double quotes hold nothing the shell
// would expand.
var message = regexp.MustCompile("(^|\\s)(-[A-Za-z]*m)\\s*(?:'[^']*'|\"[^\"$`\\\\!]*\")")

// mentionsGhOrGit is whether command names gh or git anywhere, quoted or not, as written or with its quotes and
// expansions dropped (g"i"t, g${x}it).
func mentionsGhOrGit(command string) bool {
	return ghOrGit.MatchString(command) || ghOrGit.MatchString(unquote.Replace(command)) || ghOrGit.MatchString(expansion.ReplaceAllString(command, ""))
}

// plainSafeCall is whether command is one call of gh or git, with no shell syntax, whose subcommand is in
// safeCommands and whose words hold nothing that runs another command, reads a config or file, or changes
// what the call is.
func plainSafeCall(command, base string) bool {
	if strings.HasPrefix(command, "git commit ") {
		command = message.ReplaceAllString(command, "$1$2 MSG")
	}
	if !shellSafe.MatchString(command) {
		return false
	}
	words := strings.Fields(command)
	if len(words) < 2 || words[0] != "gh" && words[0] != "git" {
		return false
	}
	if words[0] == "gh" {
		words = withoutRepoFlags(words)
	}
	if slices.ContainsFunc(words[1:], refusedWord) {
		return false
	}
	for n := min(3, len(words)); n >= 2; n-- {
		if rule, ok := safeCommands[strings.Join(words[:n], " ")]; ok {
			// gh's own search for the word merge is still held, as the denylist has it.
			return (rule == nil || rule(words[n:], base)) && !(words[0] == "gh" && denied(command, base))
		}
	}
	return false
}

// withoutRepoFlags drops gh's -R and --repo, which may stand before the subcommand: gh -R o/r pr view.
func withoutRepoFlags(words []string) []string {
	out := words[:1:1]
	for i := 1; i < len(words); i++ {
		switch w := words[i]; {
		case w == "-R" || w == "--repo":
			i++
		case strings.HasPrefix(w, "--repo="):
		default:
			out = append(out, w)
		}
	}
	return out
}

// refusedOptions are the long options with which git runs a program or writes a file.
var refusedOptions = []string{"--git-dir", "--work-tree", "--exec-path", "--exec", "--upload-pack", "--receive-pack", "--config-env", "--open-files-in-pager", "--output", "--ext-diff"}

// refusedWord is a word with which gh or git runs another command, writes a file, or loads a config, an alias or
// a path of its own choice, wherever it stands in the call.
func refusedWord(w string) bool {
	switch strings.ToLower(w) {
	case "alias", "config", "extension", "extensions":
		return true
	}
	if option, ok := strings.CutPrefix(w, "--"); ok {
		// git takes any unique start of a long option (--upl for --upload-pack), so a start of one counts too.
		name := "--" + strings.SplitN(option, "=", 2)[0]
		return len(name) > 2 && slices.ContainsFunc(refusedOptions, func(o string) bool { return strings.HasPrefix(o, name) || strings.HasPrefix(name, o) })
	}
	// -c sets a config for one call, -x runs a command; a word may hold either as the start of an option.
	return strings.HasPrefix(w, "-c") || strings.HasPrefix(w, "-x") || strings.HasPrefix(w, "~") || strings.Contains(w, "::")
}

// branchKeeps is whether a git branch call only lists or creates a branch: every option is one that reads.
func branchKeeps(args []string, _ string) bool {
	return !slices.ContainsFunc(args, func(a string) bool {
		if option, ok := strings.CutPrefix(a, "--"); ok {
			return !slices.Contains([]string{"all", "remotes", "verbose", "list", "show-current", "contains", "no-contains", "merged", "no-merged", "points-at", "sort", "format", "color", "no-color", "column", "no-column", "quiet"}, strings.SplitN(option, "=", 2)[0])
		}
		return strings.HasPrefix(a, "-") && strings.Trim(a[1:], "arvlq") != ""
	})
}

var (
	remoteName = regexp.MustCompile(`^[A-Za-z0-9_][A-Za-z0-9_.-]*$`)
	branchName = regexp.MustCompile(`^[A-Za-z0-9_][A-Za-z0-9_.-]*(/[A-Za-z0-9_.-]+)*$`)
)

// pushesOwnBranch is whether a git push names a remote and one plain branch that is not the base. A refspec
// with a colon, a +, HEAD or a ref path can land on a branch under another name, so it is an Ask.
func pushesOwnBranch(args []string, base string) bool {
	var positional []string
	for _, a := range args {
		switch {
		case !strings.HasPrefix(a, "-"):
			positional = append(positional, a)
		case slices.Contains([]string{"-u", "--set-upstream", "-v", "--verbose", "-q", "--quiet", "-n", "--dry-run", "--no-verify"}, a):
		default:
			return false
		}
	}
	if len(positional) != 2 || !remoteName.MatchString(positional[0]) || !branchName.MatchString(positional[1]) {
		return false
	}
	branch := positional[1]
	for _, prefix := range []string{"refs/", "heads/", "remotes/", "tags/"} {
		if strings.HasPrefix(branch, prefix) {
			return false
		}
	}
	return branch != "HEAD" && branch != base
}

// apiGet is whether a gh api call can only read: no method but GET, no field, input or header (a header can
// override the method), and no graphql endpoint, which takes a mutation as a query.
func apiGet(args []string, _ string) bool {
	for i := 0; i < len(args); i++ {
		a := args[i]
		switch {
		case a == "-X" || a == "--method":
			i++
			if i >= len(args) || !strings.EqualFold(args[i], "GET") {
				return false
			}
		case strings.HasPrefix(a, "-X"):
			if !strings.EqualFold(a[2:], "GET") {
				return false
			}
		case strings.HasPrefix(a, "--method="):
			if !strings.EqualFold(strings.TrimPrefix(a, "--method="), "GET") {
				return false
			}
		case slices.Contains([]string{"-i", "--include", "--paginate", "--silent", "--verbose", "--slurp"}, a):
		case a == "-q" || a == "--jq":
			i++
		case strings.HasPrefix(a, "-"):
			return false
		case slices.Contains(strings.Split(strings.ToLower(a), "/"), "graphql"):
			return false
		}
	}
	return true
}
