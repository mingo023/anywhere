package worktree

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"os/exec"
	"slices"
	"strings"
	"time"

	"pocketd/internal/registry"
)

// ErrNoGh is the failure when the login PATH has no gh.
var ErrNoGh = coded("spawn_failed", "Install GitHub CLI (gh) to open pull requests")

type PR struct {
	Number         int
	URL, Head, OID string
	Fork           bool
	State          string
}

// View asks gh, at path gh, about the pull request ref ("123", "#123" or a
// URL) of project.
func View(gh, project, ref string, env []string) (PR, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, gh, "pr", "view", "--json", "number,url,headRefName,headRefOid,isCrossRepository,state", "--", ref)
	cmd.Dir = project
	cmd.Env = append(slices.Clip(env), "GIT_TERMINAL_PROMPT=0")
	out, err := cmd.Output()
	if ctx.Err() != nil {
		return PR{}, failed("GitHub didn't answer in 15s")
	}
	if exit, ok := errors.AsType[*exec.ExitError](err); ok {
		return PR{}, ghFailure(ref, string(exit.Stderr))
	}
	if err != nil {
		return PR{}, err
	}
	return parseView(out)
}

func parseView(out []byte) (PR, error) {
	var v struct {
		Number int    `json:"number"`
		URL    string `json:"url"`
		Head   string `json:"headRefName"`
		OID    string `json:"headRefOid"`
		Fork   bool   `json:"isCrossRepository"`
		State  string `json:"state"`
	}
	if err := json.Unmarshal(out, &v); err != nil || v.Number == 0 || v.OID == "" {
		return PR{}, failed("gh answered with something other than a pull request")
	}
	return PR(v), nil
}

func ghFailure(ref, stderr string) error {
	switch {
	case strings.Contains(stderr, "gh auth login"):
		return failed("Run gh auth login to open pull requests")
	case strings.Contains(stderr, "Could not resolve"), strings.Contains(stderr, "no pull requests found"):
		return failed("No pull request %s", ref)
	}
	line, _, _ := strings.Cut(strings.TrimSpace(stderr), "\n")
	if line == "" {
		return failed("gh couldn't open pull request %s", ref)
	}
	return failed("%s", line)
}

// PreparePR readies pr's head for a new Worktree named name, or after its
// branch when name is empty: the head branch for a PR from this repo, pr/<n>
// for one from a fork.
func PreparePR(f registry.File, project, name string, pr PR, env []string) (*Open, error) {
	branch := pr.Head
	if pr.Fork {
		branch = fmt.Sprintf("pr/%d", pr.Number)
	}
	o, err := prepare(f, project, name, branch, env)
	if err != nil {
		return nil, err
	}
	remote, err := o.remoteFor(pr.URL)
	if err != nil {
		return nil, err
	}
	live, oid := false, ""
	if !pr.Fork {
		tracking := "refs/remotes/" + remote + "/" + pr.Head
		if _, err := o.git("fetch", "-q", "--no-tags", remote, "+refs/heads/"+pr.Head+":"+tracking); err == nil {
			live, oid = true, o.at(tracking)
		}
	}
	if !live {
		// A private ref, as FETCH_HEAD is shared with whatever else fetches.
		ref := fmt.Sprintf("refs/pocket/pr/%d", pr.Number)
		if _, err := o.git("fetch", "-q", "--no-tags", remote, fmt.Sprintf("+refs/pull/%d/head:%s", pr.Number, ref)); err != nil {
			return nil, err
		}
		oid = o.at(ref)
		o.git("update-ref", "-d", ref)
	}
	if oid != pr.OID {
		return nil, failed("PR #%d changed while opening, try again", pr.Number)
	}
	if !o.adopt() {
		if err := o.branchAt(oid, pr.Number); err != nil {
			return nil, err
		}
		if err := o.track(remote, pr, live); err != nil {
			o.rollback()
			return nil, err
		}
	} else if o.at("refs/heads/"+o.Branch) != oid {
		o.note(fmt.Sprintf("%s isn't at PR #%d's head", o.Branch, pr.Number))
	}
	if pr.State != "OPEN" {
		o.note(fmt.Sprintf("PR #%d is %s", pr.Number, strings.ToLower(pr.State)))
	}
	return o, nil
}

// remoteFor is the remote whose URL ends in the OWNER/REPO of prURL, origin
// first.
func (o *Open) remoteFor(prURL string) (string, error) {
	u, err := url.Parse(prURL)
	if err != nil {
		return "", err
	}
	parts := strings.Split(strings.Trim(u.Path, "/"), "/")
	if len(parts) < 2 {
		return "", failed("No remote points at %s", prURL)
	}
	repo := parts[0] + "/" + parts[1]
	out, err := o.git("remote")
	if err != nil {
		return "", err
	}
	remotes := strings.Fields(out)
	if i := slices.Index(remotes, "origin"); i > 0 {
		remotes[0], remotes[i] = remotes[i], remotes[0]
	}
	for _, r := range remotes {
		at, _ := o.git("remote", "get-url", r)
		at = strings.TrimSuffix(strings.TrimSuffix(strings.ToLower(at), "/"), ".git")
		if want := strings.ToLower(repo); strings.HasSuffix(at, "/"+want) || strings.HasSuffix(at, ":"+want) {
			return r, nil
		}
	}
	return "", failed("No remote points at %s", repo)
}

// branchAt makes Branch, or moves it forward, to oid. A Branch already at or
// past oid is kept; one with commits off the PR fails.
func (o *Open) branchAt(oid string, n int) error {
	local := o.at("refs/heads/" + o.Branch)
	switch {
	case local == "":
		if _, err := o.git("branch", "--no-track", o.Branch, oid); err != nil {
			return err
		}
		o.created, o.oid = true, oid
	case ancestor(o.Project, oid, local):
	case ancestor(o.Project, local, oid):
		if _, err := o.git("update-ref", "refs/heads/"+o.Branch, oid, local); err != nil {
			return err
		}
		o.note("Fast-forwarded " + o.Branch)
	default:
		return failed("Branch %s has commits not in PR #%d", o.Branch, n)
	}
	return nil
}

// track makes Branch pull from the PR's head branch while it lives, else from
// the PR's ref, as gh pr checkout does. A plain git push then refuses, since
// the upstream's name differs, so a fork's PR isn't pushed to the wrong place.
func (o *Open) track(remote string, pr PR, live bool) error {
	if live {
		_, err := o.git("branch", "--set-upstream-to="+remote+"/"+pr.Head, o.Branch)
		return err
	}
	if _, err := o.git("config", "branch."+o.Branch+".remote", remote); err != nil {
		return err
	}
	_, err := o.git("config", "branch."+o.Branch+".merge", fmt.Sprintf("refs/pull/%d/head", pr.Number))
	return err
}
