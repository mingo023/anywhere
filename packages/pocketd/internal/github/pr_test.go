package github

import (
	"encoding/json"
	"errors"
	"flag"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

var update = flag.Bool("update", false, "rewrite the golden files")

func fixture(t *testing.T, name string) string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("testdata", name))
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}

func mustView(t *testing.T, json string) *PR {
	t.Helper()
	pr, problem := viewResult(json, nil)
	if pr == nil || problem != "" {
		t.Fatalf("no PR in %s (%s)", json, problem)
	}
	return pr
}

type named struct{ name, outcome string }

func outcomes(runs []proto.CheckRun) []named {
	var out []named
	for _, r := range runs {
		out = append(out, named{r.Name, r.Outcome})
	}
	return out
}

func TestAnOpenPRCountsItsChecksByOutcome(t *testing.T) {
	pr := mustView(t, fixture(t, "open.json"))
	if pr.Number != 42 || pr.Title != "Fix the flaky snapshot" || pr.URL != "https://github.com/acme/app/pull/42" || pr.State != "OPEN" || pr.Draft {
		t.Fatalf("%+v", pr)
	}
	if pr.Checks != (proto.Checks{Passed: 3, Failed: 3, Pending: 2}) {
		t.Fatalf("%+v", pr.Checks)
	}
	got := outcomes(pr.Runs)
	if !reflect.DeepEqual(got[:3], []named{{"build", "passed"}, {"label", "skipped"}, {"test", "failed"}}) ||
		!reflect.DeepEqual(got[5:], []named{{"ci/circleci", "pending"}, {"deploy", "failed"}, {"vercel", "passed"}}) {
		t.Fatalf("%v", got)
	}
}

func TestAnOpenPRListsEachCheckWithItsLog(t *testing.T) {
	pr := mustView(t, fixture(t, "cli.json"))
	build := pr.Runs[1]
	if build.Name != "CodeQL-Build" || build.Outcome != "passed" || build.URL != "https://github.com/cli/cli/actions/runs/37048540763/job/110975885942" {
		t.Fatalf("%+v", build)
	}
	if pr.Runs[0].Outcome != "skipped" || pr.Runs[2].Outcome != "pending" {
		t.Fatalf("%+v", pr.Runs)
	}
}

func TestAnOpenPRReadsItsReviewAndMergeability(t *testing.T) {
	pr := mustView(t, fixture(t, "cli.json"))
	if pr.Review != "APPROVED" || pr.Mergeable != "MERGEABLE" || pr.Blocked {
		t.Fatalf("%+v", pr)
	}
	if !reflect.DeepEqual(pr.Reviews, []proto.Review{{Login: "babakks", State: "APPROVED"}, {Login: "andyfeller", State: "CHANGES_REQUESTED"}}) {
		t.Fatalf("%+v", pr.Reviews)
	}
	if !reflect.DeepEqual(pr.Requested, []string{"williammartin", "cli"}) {
		t.Fatalf("%v", pr.Requested)
	}
	if pr.Base != "bagtoad/artifact-skill-docs" || pr.Head != "5f85ec828f6d2fe28d81ba71a61b8a2181edfb6a" {
		t.Fatalf("%s %s", pr.Base, pr.Head)
	}
}

func TestARerunCheckCountsOnlyItsLatestRun(t *testing.T) {
	pr := mustView(t, fixture(t, "rerun.json"))
	if got := outcomes(pr.Runs); !reflect.DeepEqual(got, []named{{"type-label", "passed"}, {"build", "pending"}, {"build", "failed"}}) {
		t.Fatalf("%v", got)
	}
	if pr.Runs[0].URL != "https://github.com/mingo023/anywhere/actions/runs/38583280097/job/115918493199" {
		t.Fatal(pr.Runs[0].URL)
	}
	if pr.Checks != (proto.Checks{Passed: 1, Failed: 1, Pending: 1}) {
		t.Fatalf("%+v", pr.Checks)
	}
}

func TestTimestampsReadAsUnixSeconds(t *testing.T) {
	for stamp, want := range map[string]int64{"1970-01-01T00:00:00Z": 0, "2026-10-02T18:37:02Z": 1_790_966_222, "2024-02-29T12:00:00.5Z": 1_709_208_000} {
		if got, ok := epoch(stamp); !ok || got != want {
			t.Errorf("%s: %d", stamp, got)
		}
	}
	for _, stamp := range []string{"yesterday", "0001-01-01T00:00:00Z"} {
		if _, ok := epoch(stamp); ok {
			t.Errorf("%s read as a time", stamp)
		}
	}
}

func TestReviewThreadsFallBackToTheirOriginalLine(t *testing.T) {
	threads, ok := threadsResult(fixture(t, "threads.json"))
	if !ok {
		t.Fatal("no threads")
	}
	a, b := threads[0], threads[1]
	if a.ID != "PRRT_a" || a.Path != "src/sync.rs" || a.Line != 42 || a.Left || a.Resolved {
		t.Fatalf("%+v", a)
	}
	want := proto.ThreadComment{Author: "rina", Body: "This retries forever.", At: 1_790_966_222, Hunk: " fn sync() {\n+    loop {\n+        retry();"}
	if a.Comments[0] != want {
		t.Fatalf("%+v", a.Comments[0])
	}
	if b.Line != 7 || !b.Left || !b.Outdated || !b.Resolved || b.Comments[0].Author != "ghost" {
		t.Fatalf("%+v", b)
	}
	if _, ok := threadsResult(`{"errors":[{"message":"Could not resolve"}]}`); ok {
		t.Fatal("an error read as threads")
	}
}

func TestAHunkTravelsAsItsLastLinesWithoutItsHeader(t *testing.T) {
	if got := hunkTail("@@ -1,6 +1,6 @@\n a\n b\n c\n-d\n+e\n f", 4); got != " c\n-d\n+e\n f" {
		t.Fatalf("%q", got)
	}
	if hunkTail("@@ -1 +1 @@\n+a", 4) != "+a" || hunkTail("", 4) != "" {
		t.Fatal("short hunks")
	}
}

func TestAThreadReadsAsRepliedOnceSomeoneElseAnswers(t *testing.T) {
	threads, _ := threadsResult(fixture(t, "threads.json"))
	alone := threads[0]
	alone.Comments = alone.Comments[:1]
	got := []string{ThreadState(alone, false), ThreadState(threads[0], false), ThreadState(alone, true), ThreadState(threads[1], true)}
	if !reflect.DeepEqual(got, []string{"open", "replied", "sent", "resolved"}) {
		t.Fatalf("%v", got)
	}
}

func TestMergedClosedAndDraftPRsKeepTheirState(t *testing.T) {
	merged := mustView(t, `{"isDraft":false,"number":7,"state":"MERGED","title":"t","url":"u","statusCheckRollup":[]}`)
	closed := mustView(t, `{"isDraft":false,"number":8,"state":"CLOSED","title":"t","url":"u","statusCheckRollup":[]}`)
	draft := mustView(t, `{"isDraft":true,"number":9,"state":"OPEN","title":"t","url":"u","statusCheckRollup":[]}`)
	if merged.State != "MERGED" || closed.State != "CLOSED" || draft.State != "OPEN" || !draft.Draft || merged.Checks != (proto.Checks{}) {
		t.Fatal(merged, closed, draft)
	}
}

func TestAMissingGhDiffersFromALoggedOutOne(t *testing.T) {
	for _, missing := range []string{"zsh:1: command not found: gh", "bash: line 1: gh: command not found", "fish: Unknown command: gh"} {
		if _, p := viewResult("", &Failure{missing}); p != Missing {
			t.Errorf("%s: %q", missing, p)
		}
	}
	if _, p := viewResult("", ErrMissing); p != Missing {
		t.Errorf("no gh on PATH: %q", p)
	}
	loggedOut := "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token."
	if _, p := viewResult("", &Failure{loggedOut}); p != SignedOut {
		t.Errorf("%q", p)
	}
}

func TestABranchWithoutAPROrAFailingGhHasNone(t *testing.T) {
	for _, err := range []error{&Failure{`no pull requests found for branch "fix/restore-handoff"`}, &Failure{"fatal: not a git repository (or any of the parent directories): .git"}, errors.New("timed out")} {
		if pr, p := viewResult("", err); pr != nil || p != "" {
			t.Errorf("%v: %v %q", err, pr, p)
		}
	}
	if pr, p := viewResult("not json", nil); pr != nil || p != "" {
		t.Errorf("not json: %v", pr)
	}
}

func TestCreateArgvTargetsTheBaseWithoutItsRemote(t *testing.T) {
	if got := createArgv("origin/main", "Fix it", false); !reflect.DeepEqual(got, []string{"gh", "pr", "create", "--base", "main", "--title", "Fix it", "--body-file", "-"}) {
		t.Fatal(got)
	}
	if createArgv("develop", "Fix it", false)[4] != "develop" {
		t.Fatal("develop")
	}
	if got := createArgv("", "Fix it", false); slices.Contains(got, "--base") {
		t.Fatalf("no base given, yet %v", got)
	}
}

func TestADraftPRIsCreatedAsADraft(t *testing.T) {
	if got := createArgv("main", "t", true); got[len(got)-1] != "--draft" {
		t.Fatal(got)
	}
}

func TestTheCreatedPRIsReadFromItsURL(t *testing.T) {
	if n, url, ok := created("Warning: 1 uncommitted change\nhttps://github.com/acme/app/pull/57\n"); !ok || n != 57 || url != "https://github.com/acme/app/pull/57" {
		t.Fatal(n, url)
	}
	if _, _, ok := created("pull request create failed"); ok {
		t.Fatal("no URL read as one")
	}
}

func TestMergingNeverDeletesTheBranch(t *testing.T) {
	if got := mergeArgv(7, "squash"); !reflect.DeepEqual(got, []string{"gh", "pr", "merge", "7", "--squash"}) {
		t.Fatal(got)
	}
	if mergeArgv(7, "rebase")[4] != "--rebase" || mergeArgv(7, "merge")[4] != "--merge" {
		t.Fatal("methods")
	}
	if got := readyArgv(7); !reflect.DeepEqual(got, []string{"gh", "pr", "ready", "7"}) {
		t.Fatal(got)
	}
}

func TestThreadCallsPassTheirTextVerbatim(t *testing.T) {
	threads := threadsArgv(57)
	if !reflect.DeepEqual(threads[:9], []string{"gh", "api", "graphql", "-F", "owner={owner}", "-F", "name={repo}", "-F", "number=57"}) || !strings.HasPrefix(threads[10], "query=query($owner") {
		t.Fatal(threads)
	}
	reply := replyArgv("PRRT_a", "@rina done\nsee 1a2b")
	if !reflect.DeepEqual(reply[:7], []string{"gh", "api", "graphql", "-f", "id=PRRT_a", "-f", "body=@rina done\nsee 1a2b"}) {
		t.Fatal(reply)
	}
	if !strings.Contains(resolveArgv("PRRT_a", true)[6], "resolveReviewThread") || !strings.Contains(resolveArgv("PRRT_a", false)[6], "unresolveReviewThread") {
		t.Fatal("resolve")
	}
}

func TestAPRMergesOnlyOnceEveryGateIsGreen(t *testing.T) {
	ready := PR{State: "OPEN", Mergeable: "MERGEABLE", Checks: proto.Checks{Passed: 2}, Review: "APPROVED"}
	if g := ready.Gate(); g != "ready" {
		t.Fatal(g)
	}
	with := func(f func(*PR)) PR {
		pr := ready
		f(&pr)
		return pr
	}
	for _, c := range []struct {
		pr   PR
		gate string
	}{
		{with(func(p *PR) { p.Draft = true }), "draft"},
		{with(func(p *PR) { p.Mergeable, p.Checks = "CONFLICTING", proto.Checks{Failed: 1} }), "conflicts"},
		{with(func(p *PR) { p.Checks = proto.Checks{Failed: 1, Pending: 1} }), "checksFailed"},
		{with(func(p *PR) { p.Checks = proto.Checks{Pending: 1} }), "checksRunning"},
		{with(func(p *PR) { p.Review = "CHANGES_REQUESTED" }), "changesRequested"},
		{with(func(p *PR) { p.Review = "REVIEW_REQUIRED" }), "reviewRequired"},
		{with(func(p *PR) { p.Blocked = true }), "blocked"},
		{with(func(p *PR) { p.State = "MERGED" }), "done"},
	} {
		if g := c.pr.Gate(); g != c.gate {
			t.Errorf("%+v: %s, want %s", c.pr, g, c.gate)
		}
	}
}

func TestAnUnknownMergeabilityNeverReadsAsNoConflicts(t *testing.T) {
	pr := PR{State: "OPEN", Mergeable: "UNKNOWN", Review: "APPROVED"}
	if g := pr.Gate(); g != "checking" {
		t.Fatal(g)
	}
}

const versionOut = "gh version 2.81.0 (2025-10-01)\nhttps://github.com/cli/cli/releases/tag/v2.81.0\n"

func ready(login string, accounts ...string) Status {
	return Status{State: "ready", Version: "2.81.0", Login: login, Accounts: accounts}
}

func TestGhStatusReadsTheVersionEveryAccountAndTheActiveOne(t *testing.T) {
	auth := "github.com\n  ✓ Logged in to github.com account old-me (keyring)\n  - Active account: false\n  ✓ Logged in to github.com account minh-ngo (keyring)\n  - Active account: true\n  - Git operations protocol: https\n"
	if got := status(versionOut, nil, auth, nil); !reflect.DeepEqual(got, ready("minh-ngo", "old-me", "minh-ngo")) {
		t.Fatalf("%+v", got)
	}
	old := "github.com\n  ✓ Logged in to github.com as minh-ngo (oauth_token)\n"
	if got := status(versionOut, nil, old, nil); !reflect.DeepEqual(got, ready("minh-ngo", "minh-ngo")) {
		t.Fatalf("%+v", got)
	}
}

func TestGhStatusListsThreeAccountsWithTheFirstActive(t *testing.T) {
	auth := "github.com\n  ✓ Logged in to github.com account mingo023 (keyring)\n  - Active account: true\n  - Git operations protocol: https\n  - Token: gho_************************************\n  - Token scopes: 'gist', 'read:org', 'repo', 'workflow'\n\n  ✓ Logged in to github.com account hodfords-minhngo-be (keyring)\n  - Active account: false\n  - Git operations protocol: https\n  - Token: gho_************************************\n\n  ✓ Logged in to github.com account minhngovan-regask (keyring)\n  - Active account: false\n"
	if got := status(versionOut, nil, auth, nil); !reflect.DeepEqual(got, ready("mingo023", "mingo023", "hodfords-minhngo-be", "minhngovan-regask")) {
		t.Fatalf("%+v", got)
	}
}

func TestGhStatusTellsAMissingGhFromASignedOutOne(t *testing.T) {
	if got := status("", ErrMissing, "", errors.New("")); got.State != "missing" {
		t.Fatalf("%+v", got)
	}
	signedOut := "You are not logged into any GitHub hosts. To log in, run: gh auth login\n"
	if got := status(versionOut, nil, signedOut, &Failure{signedOut}); !reflect.DeepEqual(got, Status{State: "signedOut", Version: "2.81.0"}) {
		t.Fatalf("%+v", got)
	}
}

func TestAPRTravelsAsTheGoldenWirePR(t *testing.T) {
	pr := mustView(t, fixture(t, "cli.json"))
	pr.Threads, _ = threadsResult(fixture(t, "threads.json"))
	got, _ := json.MarshalIndent(proto.NewGhPR("p", "/Users/me/dev/cli", "", pr.Wire()), "", "  ")
	path := filepath.Join("testdata", "golden", "cli_pr.json")
	if *update {
		os.WriteFile(path, append(got, '\n'), 0o644)
	}
	want, err := os.ReadFile(path)
	if err != nil || strings.TrimSpace(string(want)) != string(got) {
		t.Fatalf("%v\n%s", err, got)
	}
}
