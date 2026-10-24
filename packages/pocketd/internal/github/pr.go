// Package github asks the gh CLI about the pull request of each Worktree's
// branch and acts on it, as the desktop does (packages/desktop/crates/git/src/github.rs).
package github

import (
	"encoding/json"
	"strconv"
	"strings"
	"time"

	"pocketd/internal/proto"
)

// PR is a pull request as `gh pr view` reports it, with its review threads.
type PR struct {
	Number int
	Title  string
	URL    string
	// State is OPEN, MERGED or CLOSED.
	State string
	Draft bool
	// Checks counts Runs by outcome; skipped ones count as passed.
	Checks proto.Checks
	Runs   []proto.CheckRun
	// Review is APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED or "".
	Review string
	// Reviews are each reviewer's latest verdict, as GitHub names it.
	Reviews   []proto.Review
	Requested []string
	// Mergeable is MERGEABLE, CONFLICTING or UNKNOWN: GitHub is still working
	// it out, as it does for a while after every push.
	Mergeable string
	// Blocked: branch protection holds the merge back.
	Blocked bool
	Base    string
	// Head is the oid of the PR's head commit.
	Head    string
	Threads []proto.Thread
}

// Problem is why gh can't be asked at all, as opposed to a branch having no PR.
type Problem string

const (
	Missing   Problem = "missing"
	SignedOut Problem = "signedOut"
)

// view is the PR for the branch checked out in the working directory.
var view = []string{"gh", "pr", "view", "--json", "number,title,url,state,isDraft,statusCheckRollup,reviewDecision,latestReviews,reviewRequests,mergeable,mergeStateStatus,headRefOid,baseRefName"}

// viewResult reads what view returned. No PR, or any other failure, is nil.
func viewResult(out string, err error) (*PR, Problem) {
	if err != nil {
		return nil, problemOf(err)
	}
	return parse(out), ""
}

// problemOf tells a missing or signed-out gh from any other failure.
func problemOf(err error) Problem {
	if err == nil {
		return ""
	}
	e := err.Error()
	switch {
	// A login shell reports a missing command: zsh and bash say "command not found", fish "Unknown command".
	case err == ErrMissing, strings.Contains(e, "command not found"), strings.Contains(e, "Unknown command"):
		return Missing
	case strings.Contains(e, "gh auth login"):
		return SignedOut
	}
	return ""
}

type rollupEntry struct {
	Typename     string `json:"__typename"`
	State        string `json:"state"`
	Context      string `json:"context"`
	TargetURL    string `json:"targetUrl"`
	Name         string `json:"name"`
	Status       string `json:"status"`
	Conclusion   string `json:"conclusion"`
	DetailsURL   string `json:"detailsUrl"`
	StartedAt    string `json:"startedAt"`
	WorkflowName string `json:"workflowName"`
}

type login struct {
	Login *string `json:"login"`
}

func parse(out string) *PR {
	var v struct {
		Number         *int          `json:"number"`
		Title          *string       `json:"title"`
		URL            *string       `json:"url"`
		State          *string       `json:"state"`
		IsDraft        bool          `json:"isDraft"`
		Rollup         []rollupEntry `json:"statusCheckRollup"`
		ReviewDecision string        `json:"reviewDecision"`
		LatestReviews  []struct {
			Author *login `json:"author"`
			State  string `json:"state"`
		} `json:"latestReviews"`
		ReviewRequests []struct {
			Login, Slug, Name string
		} `json:"reviewRequests"`
		Mergeable        string `json:"mergeable"`
		MergeStateStatus string `json:"mergeStateStatus"`
		HeadRefOid       string `json:"headRefOid"`
		BaseRefName      string `json:"baseRefName"`
	}
	if json.Unmarshal([]byte(out), &v) != nil || v.Number == nil || v.Title == nil || v.URL == nil || v.State == nil {
		return nil
	}
	pr := &PR{Number: *v.Number, Title: *v.Title, URL: *v.URL, State: "OPEN", Draft: v.IsDraft, Base: v.BaseRefName, Head: v.HeadRefOid, Mergeable: "UNKNOWN"}
	if *v.State == "MERGED" || *v.State == "CLOSED" {
		pr.State = *v.State
	}
	pr.Runs = runs(v.Rollup)
	for _, r := range pr.Runs {
		switch r.Outcome {
		case "failed":
			pr.Checks.Failed++
		case "pending":
			pr.Checks.Pending++
		default:
			pr.Checks.Passed++
		}
	}
	switch v.ReviewDecision {
	case "APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED":
		pr.Review = v.ReviewDecision
	}
	for _, r := range v.LatestReviews {
		pr.Reviews = append(pr.Reviews, proto.Review{Login: author(r.Author), State: r.State})
	}
	for _, r := range v.ReviewRequests {
		if who := firstOf(r.Login, r.Slug, r.Name); who != "" {
			pr.Requested = append(pr.Requested, who)
		}
	}
	if v.Mergeable == "MERGEABLE" || v.Mergeable == "CONFLICTING" {
		pr.Mergeable = v.Mergeable
	}
	pr.Blocked = v.MergeStateStatus == "BLOCKED"
	return pr
}

func firstOf(s ...string) string {
	for _, v := range s {
		if v != "" {
			return v
		}
	}
	return ""
}

// author is a login; a deleted account has none.
func author(a *login) string {
	if a == nil || a.Login == nil {
		return "ghost"
	}
	return *a.Login
}

// runs is the latest run of each check: re-running one, as a label change
// does, adds a run on the same head and leaves the old one in the rollup.
func runs(rollup []rollupEntry) []proto.CheckRun {
	type key struct{ workflow, name string }
	type seen struct {
		at      int
		started int64
	}
	var out []proto.CheckRun
	latest := map[key]seen{}
	for _, c := range rollup {
		var run proto.CheckRun
		// Commit statuses carry state; check runs carry status and conclusion.
		if c.Typename == "StatusContext" {
			run = proto.CheckRun{Name: c.Context, Outcome: "passed", URL: c.TargetURL}
			switch c.State {
			case "FAILURE", "ERROR":
				run.Outcome = "failed"
			case "PENDING", "EXPECTED":
				run.Outcome = "pending"
			}
		} else {
			run = proto.CheckRun{Name: c.Name, Outcome: "passed", URL: c.DetailsURL}
			switch {
			case c.Conclusion == "FAILURE", c.Conclusion == "TIMED_OUT", c.Conclusion == "CANCELLED", c.Conclusion == "ACTION_REQUIRED", c.Conclusion == "STARTUP_FAILURE":
				run.Outcome = "failed"
			case c.Status != "COMPLETED":
				run.Outcome = "pending"
			case c.Conclusion == "SKIPPED", c.Conclusion == "NEUTRAL":
				run.Outcome = "skipped"
			}
		}
		// A queued run has no start yet and is the newest.
		started, ok := epoch(c.StartedAt)
		if !ok {
			started = 1<<63 - 1
		}
		k := key{c.WorkflowName, run.Name}
		switch s, had := latest[k]; {
		case !had:
			latest[k] = seen{len(out), started}
			out = append(out, run)
		case started >= s.started:
			out[s.at] = run
			latest[k] = seen{s.at, started}
		}
	}
	return out
}

// epoch reads 2026-10-02T18:37:02Z as Unix seconds. GitHub's zero time,
// 0001-01-01, is no time, as the desktop's unsigned stamps read it.
func epoch(stamp string) (int64, bool) {
	t, err := time.Parse(time.RFC3339, stamp)
	if err != nil || t.Unix() < 0 {
		return 0, false
	}
	return t.Unix(), true
}

// Gate is why the merge button is off, the first thing that holds it back,
// by protocol.md's names.
func (pr *PR) Gate() string {
	switch {
	case pr.State != "OPEN":
		return "done"
	case pr.Draft:
		return "draft"
	case pr.Mergeable == "CONFLICTING":
		return "conflicts"
	case pr.Checks.Failed > 0:
		return "checksFailed"
	case pr.Checks.Pending > 0:
		return "checksRunning"
	case pr.Review == "CHANGES_REQUESTED":
		return "changesRequested"
	case pr.Review == "REVIEW_REQUIRED":
		return "reviewRequired"
	case pr.Mergeable != "MERGEABLE":
		return "checking"
	case pr.Blocked:
		return "blocked"
	}
	return "ready"
}

// ThreadState is how far a review thread has got: open, replied, sent or resolved.
// sent is when it was handed to an agent since the last push.
func ThreadState(t proto.Thread, sent bool) string {
	switch {
	case t.Resolved:
		return "resolved"
	case sent:
		return "sent"
	case len(t.Comments) > 0 && t.Comments[len(t.Comments)-1].Author != t.Comments[0].Author:
		return "replied"
	}
	return "open"
}

// Wire is pr as gh.pr sends it.
func (pr *PR) Wire() *proto.PR {
	return &proto.PR{
		Number: pr.Number, Title: pr.Title, URL: pr.URL, State: pr.State, Draft: pr.Draft, Base: pr.Base, Head: pr.Head,
		Checks: pr.Checks, Runs: pr.Runs, Review: pr.Review, Reviews: pr.Reviews, Requested: pr.Requested,
		Mergeable: pr.Mergeable, Gate: pr.Gate(), Threads: pr.Threads,
	}
}

// Item is pr as gh.prs lists it for tree.
func (pr *PR) Item(tree string) proto.PRItem {
	return proto.PRItem{Tree: tree, Number: pr.Number, Title: pr.Title, State: pr.State, Draft: pr.Draft, Gate: pr.Gate(), Checks: pr.Checks}
}

const threadsQuery = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{id isResolved isOutdated path line originalLine diffSide comments(first:50){nodes{author{login} body createdAt diffHunk}}}}}}}"
const replyQuery = "mutation($id:ID!,$body:String!){addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$id,body:$body}){comment{id}}}"
const resolveQuery = "mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{isResolved}}}"
const unresolveQuery = "mutation($id:ID!){unresolveReviewThread(input:{threadId:$id}){thread{isResolved}}}"

// threadsArgv asks for PR number's review threads; gh fills in the owner and
// name of the repo it runs in.
func threadsArgv(number int) []string {
	return []string{"gh", "api", "graphql", "-F", "owner={owner}", "-F", "name={repo}", "-F", "number=" + strconv.Itoa(number), "-f", "query=" + threadsQuery}
}

func replyArgv(thread, body string) []string {
	return []string{"gh", "api", "graphql", "-f", "id=" + thread, "-f", "body=" + body, "-f", "query=" + replyQuery}
}

func resolveArgv(thread string, resolved bool) []string {
	q := unresolveQuery
	if resolved {
		q = resolveQuery
	}
	return []string{"gh", "api", "graphql", "-f", "id=" + thread, "-f", "query=" + q}
}

// hunkLines is how much of a comment's hunk travels: the lines it ends on,
// one more than the desktop shows (hunk_tail).
const hunkLines = 4

// threadsResult reads what threadsArgv printed; false when it isn't threads.
func threadsResult(out string) ([]proto.Thread, bool) {
	var v struct {
		Data *struct {
			Repository *struct {
				PullRequest *struct {
					ReviewThreads *struct {
						Nodes []struct {
							ID           string `json:"id"`
							IsResolved   bool   `json:"isResolved"`
							IsOutdated   bool   `json:"isOutdated"`
							Path         string `json:"path"`
							Line         *int   `json:"line"`
							OriginalLine *int   `json:"originalLine"`
							DiffSide     string `json:"diffSide"`
							Comments     struct {
								Nodes []struct {
									Author    *login `json:"author"`
									Body      string `json:"body"`
									CreatedAt string `json:"createdAt"`
									DiffHunk  string `json:"diffHunk"`
								} `json:"nodes"`
							} `json:"comments"`
						} `json:"nodes"`
					} `json:"reviewThreads"`
				} `json:"pullRequest"`
			} `json:"repository"`
		} `json:"data"`
	}
	if json.Unmarshal([]byte(out), &v) != nil || v.Data == nil || v.Data.Repository == nil || v.Data.Repository.PullRequest == nil || v.Data.Repository.PullRequest.ReviewThreads == nil {
		return nil, false
	}
	threads := []proto.Thread{}
	for _, t := range v.Data.Repository.PullRequest.ReviewThreads.Nodes {
		th := proto.Thread{ID: t.ID, Path: t.Path, Left: t.DiffSide == "LEFT", Outdated: t.IsOutdated, Resolved: t.IsResolved, Comments: []proto.ThreadComment{}}
		if t.Line != nil {
			th.Line = *t.Line
		} else if t.OriginalLine != nil {
			th.Line = *t.OriginalLine
		}
		for _, c := range t.Comments.Nodes {
			at, _ := epoch(c.CreatedAt)
			th.Comments = append(th.Comments, proto.ThreadComment{Author: author(c.Author), Body: c.Body, At: at, Hunk: hunkTail(c.DiffHunk, hunkLines)})
		}
		threads = append(threads, th)
	}
	return threads, true
}

// hunkTail is the last n lines of a hunk, without its header.
func hunkTail(hunk string, n int) string {
	var lines []string
	for l := range strings.Lines(hunk) {
		if l = strings.TrimSuffix(l, "\n"); !strings.HasPrefix(l, "@@") {
			lines = append(lines, l)
		}
	}
	return strings.Join(lines[max(0, len(lines)-n):], "\n")
}

// baseBranch is the branch name on the remote, without origin/.
func baseBranch(base string) string { return strings.TrimPrefix(base, "origin/") }

// createArgv opens a PR for the pushed, checked-out branch, its body read
// from stdin; without a base gh picks the repo's default branch.
func createArgv(base, title string, draft bool) []string {
	argv := []string{"gh", "pr", "create"}
	if base != "" {
		argv = append(argv, "--base", baseBranch(base))
	}
	argv = append(argv, "--title", title, "--body-file", "-")
	if draft {
		argv = append(argv, "--draft")
	}
	return argv
}

// created is the new PR's number and URL in what createArgv printed.
func created(out string) (int, string, bool) {
	lines := strings.Split(strings.TrimSpace(out), "\n")
	for i := len(lines) - 1; i >= 0; i-- {
		l := strings.TrimSpace(lines[i])
		if _, n, ok := strings.Cut(l, "/pull/"); ok {
			if number, err := strconv.Atoi(n); err == nil {
				return number, l, true
			}
		}
	}
	return 0, "", false
}

// mergeArgv merges PR number, leaving its branch alone: the worktree that has
// it checked out goes separately.
func mergeArgv(number int, method string) []string {
	return []string{"gh", "pr", "merge", strconv.Itoa(number), "--" + method}
}

func readyArgv(number int) []string { return []string{"gh", "pr", "ready", strconv.Itoa(number)} }
