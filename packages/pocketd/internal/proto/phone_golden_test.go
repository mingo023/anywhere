package proto

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

var phoneServerGolden = map[string]any{
	"terminal_list": NewTerminalList("tl", []Terminal{
		{ID: "t1", Cwd: "/Users/me/dev/pocket", Cols: 120, Rows: 40, Title: "claude", AgentID: "a1"},
		{ID: "t2", Cwd: "/Users/me/dev/pocket", Cols: 80, Rows: 24, LocalID: "l1", Exited: true},
	}),
	"terminal_list_push":    NewTerminalList("", nil),
	"terminal_attached":     NewTerminalAttached("ta", "t1", 120, 40),
	"terminal_resized":      NewTerminalResized("t1", 48, 30),
	"terminal_exited":       NewTerminalExited("t1", 0),
	"terminal_created":      NewTerminalCreated("tc", "r1", "t3"),
	"error_terminals_off":   NewErrorCode("ti", CodeTerminalsOff, "Terminals are off for phones on this Mac"),
	"error_not_implemented": NewErrorCode("gs", CodeNotImplemented, "git.status isn't served by this pocketd yet"),
	"error_not_a_tree":      NewErrorCode("gs", CodeNotATree, "This folder isn't a project on your Mac"),
	"project_list_push":     NewProjectList("", nil),
	"worktree_progress":     NewWorktreeProgress("wc", "r1", StepFetch, "Couldn't fetch, using local main"),
	"worktree_created":      NewWorktreeCreated("wc", "r1", "/Users/me/.worktrees/pocket/calm-otter", "calm-otter"),
	"branch_list": NewBranchList("bl", "main",
		[]Branch{{Name: "main", At: 1790000000}, {Name: "fix-login", At: 1789990000}},
		[]Branch{{Name: "origin/main", At: 1790000000}}),
	"branch_list_detached": NewBranchList("bl", "", nil, nil),
	"account_list": NewAccountList("al", []Account{
		{ID: "3f9a0c1b2d4e", Provider: "claude", Name: "Work", Plan: "max", Email: "me@example.com", SignedIn: true, IsDefault: true,
			FiveHour: &Usage{Pct: 42.5, ResetsAt: 1790003600}, Week: &Usage{Pct: 12}},
		{ID: "builtin", Provider: "codex", Name: "Codex", SignedIn: false},
	}),
	"git_status": NewGitStatus(GitStatus{ID: "gs", Tree: "/Users/me/dev/pocket", Rev: "9f2c", Branch: "fix-login", Upstream: "origin/fix-login", Base: "origin/main",
		Ahead: 2, Behind: 1, Unpushed: new(1),
		Totals: GitTotals{Files: 3, Added: 12, Removed: 4, Staged: 1},
		Files: []GitFile{
			{Path: "README.md", Status: "M", Staged: true, Added: new(2), Removed: new(1)},
			{Path: "src/new.rs", OldPath: "src/old.rs", Status: "R", Unstaged: true, Added: new(10), Removed: new(3)},
			{Path: "shot.png", Status: "?", Unstaged: true, Binary: true},
		},
		Commits:    []CommitRef{{Sha: "1b59160", Subject: "Fix login"}},
		NextCursor: new(500),
	}),
	"git_status_detached": NewGitStatus(GitStatus{ID: "gs", Tree: "/Users/me/dev/pocket", Rev: "0", Detached: true}),
	"git_changed":         NewGitChanged("/Users/me/dev/pocket", "a71e"),
	"gh_changed":          NewGhChanged("/Users/me/dev/pocket"),
	"git_diff": NewGitDiff(GitDiff{ID: "gd", Tree: "/Users/me/dev/pocket", Path: "src/main.rs", Language: "rust", Added: 1, Removed: 1, Lines: []DiffRow{
		{K: "h", T: "@@ -1,2 +1,2 @@ fn main()"},
		{K: "c", O: 1, N: 1, T: "fn main() {"},
		{K: "d", O: 2, T: `    println!("hi");`, W: [][2]int{{14, 16}}},
		{K: "a", N: 2, T: `    println!("hello");`, W: [][2]int{{14, 19}}},
		{K: "a", N: 3, T: strings.Repeat("x", 16), Cut: true},
	}, Truncated: &Truncated{Shown: 5, Total: 9000}}),
	"git_diff_binary": NewGitDiff(GitDiff{ID: "gd", Tree: "/Users/me/dev/pocket", Path: "shot.png", OldPath: "old.png", Language: "text", Binary: true, Image: true}),
	"git_log": NewGitLog("gl", LogTips{Head: "1b59160", Branch: "fix-login", Upstream: "a42641f", Base: "214c24c"}, []LogCommit{
		{Sha: "1b59160", Parents: []string{"214c24c"}, Author: "Minh", At: 1790000000, Subject: "Fix login"},
		{Sha: "214c24c", Author: "Minh", At: 1789990000, Subject: "Root"},
	}, true),
	"git_commit": NewGitCommit(GitCommit{ID: "gc", Sha: "1b59160", Subject: "Fix login", Body: "Closes #12", Author: "Minh", At: 1790000000,
		Files: []CommitFile{{Path: "src/main.rs", Status: "M", Added: new(1), Removed: new(1)}, {Path: "logo.png", Status: "A"}}}),
	"git_committed":     NewGitCommitted("gcc", "1b59160"),
	"error_git_failed":  NewCodedError("gp", CodeGitFailed, "! [rejected] fix-login -> fix-login (non-fast-forward)", "To github.com:me/pocket.git\n ! [rejected] fix-login -> fix-login (non-fast-forward)"),
	"gh_status":         NewGhStatus("hs", "ready", "me"),
	"gh_status_missing": NewGhStatus("hs", "missing", ""),
	"gh_pr": NewGhPR("hp", "/Users/me/dev/pocket", "", &PR{Number: 118, Title: "Fix login", URL: "https://github.com/me/pocket/pull/118", State: "OPEN",
		Base: "main", Head: "fix-login", Checks: Checks{Passed: 3, Failed: 1, Pending: 1},
		Runs:   []CheckRun{{Name: "test", Outcome: "failed", URL: "https://github.com/me/pocket/actions/runs/1"}, {Name: "lint", Outcome: "passed"}},
		Review: "CHANGES_REQUESTED", Reviews: []Review{{Login: "ana", State: "CHANGES_REQUESTED"}}, Requested: []string{"bo"},
		Mergeable: "MERGEABLE", Gate: "checksFailed",
		Threads: []Thread{{ID: "PRRT_kwDOA1", Path: "src/main.rs", Line: 2, Outdated: false, Resolved: false,
			Comments: []ThreadComment{{Author: "ana", Body: "Say hello", At: 1790000000, Hunk: "@@ -1,2 +1,2 @@\n-    println!(\"hi\");"}}}},
	}),
	"gh_pr_none":       NewGhPR("hp", "/Users/me/dev/pocket", "", nil),
	"gh_pr_problem":    NewGhPR("hp", "/Users/me/dev/pocket", "signedOut", nil),
	"gh_prs":           NewGhPRs("hps", []PRItem{{Tree: "/Users/me/.worktrees/pocket/calm-otter", Number: 118, Title: "Fix login", State: "OPEN", Draft: true, Gate: "draft", Checks: Checks{Pending: 2}}}),
	"gh_pr_created":    NewGhPRCreated("hc", 119, "https://github.com/me/pocket/pull/119"),
	"error_gh_missing": NewErrorCode("hp", CodeGhMissing, "Install the GitHub CLI on your Mac"),
	"fs_list": NewFsList("fl", "/Users/me/dev/pocket", "", []FsEntry{
		{Name: "src", Dir: true, Size: 128, Mtime: 1790000000000, Git: "M"},
		{Name: "node_modules", Dir: true, Size: 64, Mtime: 1790000000000, Ignored: true},
		{Name: "README.md", Size: 2048, Mtime: 1790000000000, Git: "M"},
		{Name: "link", Link: true, Size: 12, Mtime: 1790000000000},
	}),
	"fs_read_text": NewFsRead(FsRead{ID: "fr", Tree: "/Users/me/dev/pocket", Path: "src/main.rs", Kind: "text", Size: 30, Mtime: 1790000000000,
		Text: new("fn main() {\n    println!(\"hi\");\n}\n"), Language: "rust"}),
	"fs_read_image": NewFsRead(FsRead{ID: "fr", Tree: "/Users/me/dev/pocket", Path: "logo.png", Kind: "image", Size: 8, Mtime: 1790000000000,
		Mime: "image/png", Data: []byte("\x89PNG\r\n\x1a\n")}),
	"fs_read_too_large": NewFsRead(FsRead{ID: "fr", Tree: "/Users/me/dev/pocket", Path: "big.log", Kind: "tooLarge", Size: 900000000, Mtime: 1790000000000}),
	"fs_search":         NewFsSearch("fs", []string{"src/main.rs", "src/main_test.rs"}),
	"attach_done":       NewAttachDone("ap", "u1", "/Users/me/.pocket/attachments/u1.png"),
}

// TestEveryPhoneVerbHasAClientGolden keeps the phone's mirror and packages/protocol in step with the decoder.
func TestEveryPhoneVerbHasAClientGolden(t *testing.T) {
	files, _ := filepath.Glob(filepath.Join("testdata", "golden", "client", "*.json"))
	var types []string
	for _, f := range files {
		raw, _ := os.ReadFile(f)
		var m struct{ Type string }
		json.Unmarshal(raw, &m)
		types = append(types, m.Type)
	}
	for _, verb := range PhoneVerbs() {
		if !slices.Contains(types, verb) {
			t.Errorf("%s has no client golden", verb)
		}
	}
}

func TestPhoneCapsWaitForTheirHandlers(t *testing.T) {
	for _, c := range []string{CapFiles, CapPush} {
		if slices.Contains(ServerCaps, c) {
			t.Errorf("%s is served before its handlers land", c)
		}
	}
	if !slices.Contains(ServerCaps, CapWorkspace) {
		t.Error("workspace.v1's handlers landed but it isn't served")
	}
}

func TestDecodeClientRejectsMalformedPhoneVerbs(t *testing.T) {
	const key = `"q83vEjRWeJCrze8SNFZ4kKvN7xI0VniQq83vEjRWeJA="`
	const handle = `{"handle":"h","secret":"s","expiresAt":1800000000}`
	const prefs = `{"needsYou":true,"done":true,"failed":true,"automations":true,"details":true}`
	for _, raw := range []string{
		`{"type":"agent.timeline","id":"1","agentId":"a","beforeSeq":5,"sinceSeq":1}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","beforeSeq":"5"}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","beforeSeq":null}`,
		`{"type":"config.set","id":"1","key":"phone.terminals","value":"yes"}`,
		`{"type":"config.set","id":"1","key":"push.relay","value":""}`,
		`{"type":"terminal.list"}`,
		`{"type":"terminal.attach","id":"1"}`,
		`{"type":"terminal.attach","id":"1","terminalId":""}`,
		`{"type":"terminal.input","id":"1","terminalId":"t1","data":"not base64!"}`,
		`{"type":"terminal.input","id":"1","terminalId":"t1"}`,
		`{"type":"terminal.resize","id":"1","terminalId":"t1","cols":0,"rows":24}`,
		`{"type":"terminal.resize","id":"1","terminalId":"t1","cols":80.5,"rows":24}`,
		`{"type":"terminal.resize","id":"1","terminalId":"t1","cols":80}`,
		`{"type":"terminal.resize","id":"1","terminalId":"t1","cols":80,"rows":1001}`,
		`{"type":"terminal.create","id":"1"}`,
		`{"type":"terminal.create","id":"1","requestId":"` + strings.Repeat("r", 65) + `"}`,
		`{"type":"terminal.create","id":"1","requestId":"r","localId":"/x"}`,
		`{"type":"terminal.create","id":"1","requestId":"r","cols":-1}`,
		`{"type":"terminal.close","id":"1","terminalId":null}`,
		`{"type":"worktree.create","id":"1","requestId":"r","project":"/p"}`,
		`{"type":"worktree.create","id":"1","requestId":"r","project":"","name":"n"}`,
		`{"type":"worktree.create","id":"1","requestId":"r","project":"/p","name":""}`,
		`{"type":"worktree.create","id":"1","requestId":"r","project":"/p","name":"n","base":"main","branch":"b"}`,
		`{"type":"worktree.create","id":"1","requestId":"r","project":"/p","name":"n","pr":"118"}`,
		`{"type":"worktree.create","id":"1","requestId":"r","project":"/p","name":"n","pr":0}`,
		`{"type":"worktree.create","id":"1","requestId":"r","project":"/p","name":"n","setup":"yes"}`,
		`{"type":"worktree.delete","id":"1","path":""}`,
		`{"type":"worktree.delete","id":"1","path":"/w","deleteBranch":1}`,
		`{"type":"branch.list","id":"1"}`,
		`{"type":"account.list","id":"1","provider":"gemini"}`,
		`{"type":"agent.account","id":"1","agentId":"a"}`,
		`{"type":"agent.account","id":"1","agentId":"a","account":""}`,
		`{"type":"git.status","id":"1"}`,
		`{"type":"git.status","id":"1","tree":"/p","limit":2001}`,
		`{"type":"git.status","id":"1","tree":"/p","limit":0}`,
		`{"type":"git.status","id":"1","tree":"/p","cursor":-1}`,
		`{"type":"git.status","id":"1","tree":"/p","cursor":1.5}`,
		`{"type":"git.watch","id":"1"}`,
		`{"type":"git.watch","id":"1","trees":[""]}`,
		`{"type":"git.watch","id":"1","trees":"/p"}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a"}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"head"}}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"commit"}}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"working","rev":"x"}}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"base","Rev":"x"}}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"base","rev":null}}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"working"},"context":-1}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"working"},"maxLines":0}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"","at":{"kind":"working"}}`,
		`{"type":"git.log","id":"1","tree":"/p","limit":201}`,
		`{"type":"git.log","id":"1","tree":"/p","skip":-1}`,
		`{"type":"git.commit","id":"1","tree":"/p","sha":""}`,
		`{"type":"git.stage","id":"1","tree":"/p","paths":["a"]}`,
		`{"type":"git.stage","id":"1","tree":"/p","paths":[],"staged":true}`,
		`{"type":"git.stage","id":"1","tree":"/p","paths":[""],"staged":true}`,
		`{"type":"git.discard","id":"1","tree":"/p","paths":[null]}`,
		`{"type":"git.commit.create","id":"1","tree":"/p"}`,
		`{"type":"git.push","id":"1","tree":"/p","lease":"yes"}`,
		`{"type":"git.pull","id":"1"}`,
		`{"type":"gh.status","id":"1","tree":null}`,
		`{"type":"gh.pr","id":"1"}`,
		`{"type":"gh.prs","id":"1","project":""}`,
		`{"type":"gh.pr.create","id":"1","tree":"/p","title":"","body":"b"}`,
		`{"type":"gh.pr.create","id":"1","tree":"/p","title":"t"}`,
		`{"type":"gh.pr.merge","id":"1","tree":"/p","number":1,"method":"fast-forward"}`,
		`{"type":"gh.pr.merge","id":"1","tree":"/p","number":0,"method":"squash"}`,
		`{"type":"gh.pr.ready","id":"1","tree":"/p"}`,
		`{"type":"gh.thread.reply","id":"1","tree":"/p","thread":"t","body":""}`,
		`{"type":"gh.thread.resolve","id":"1","tree":"/p","thread":"t"}`,
		`{"type":"fs.list","id":"1","tree":"/p"}`,
		`{"type":"fs.read","id":"1","tree":"/p","path":"a","max":0}`,
		`{"type":"fs.search","id":"1","tree":"/p"}`,
		`{"type":"fs.search","id":"1","tree":"/p","query":"q","limit":501}`,
		`{"type":"attach.put","id":"1","uploadId":"u/1","name":"a.png","index":0,"data":"","last":true}`,
		`{"type":"attach.put","id":"1","uploadId":"u1","name":"a.png","index":-1,"data":"","last":true}`,
		`{"type":"attach.put","id":"1","uploadId":"u1","name":"a.png","index":0,"data":"","last":null}`,
		`{"type":"attach.put","id":"1","uploadId":"u1","name":"","index":0,"data":"","last":true}`,
		`{"type":"push.register","id":"1","key":"c2hvcnQ=","env":"production","alert":` + handle + `,"prefs":` + prefs + `}`,
		`{"type":"push.register","id":"1","key":` + key + `,"env":"sandbox","alert":` + handle + `,"prefs":` + prefs + `}`,
		`{"type":"push.register","id":"1","key":` + key + `,"env":"production","prefs":` + prefs + `}`,
		`{"type":"push.register","id":"1","key":` + key + `,"env":"production","alert":{"handle":"h","secret":"s"},"prefs":` + prefs + `}`,
		`{"type":"push.register","id":"1","key":` + key + `,"env":"production","alert":{"handle":"h","secret":"s","expiresAt":1.5},"prefs":` + prefs + `}`,
		`{"type":"push.register","id":"1","key":` + key + `,"env":"production","alert":{"handle":"","secret":"s","expiresAt":1},"prefs":` + prefs + `}`,
		`{"type":"push.register","id":"1","key":` + key + `,"env":"production","alert":` + handle + `,"prefs":{"needsYou":true}}`,
		`{"type":"push.register","id":"1","key":` + key + `,"env":"production","alert":` + handle + `,"prefs":` + prefs + `,"start":null}`,
		`{"type":"push.activity","id":"1","activityId":""}`,
		`{"type":"push.activity","id":"1","activityId":"a","handle":{"Handle":"h","secret":"s","expiresAt":1}}`,
		`{"type":"push.prefs","id":"1","prefs":{"needsYou":true,"done":true,"failed":true,"automations":true,"details":true,"sound":true}}`,
		`{"type":"push.prefs","id":"1"}`,
		`{"type":"push.test"}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", raw, err)
		}
	}
}

func TestDecodeClientAcceptsEdgesOfPhoneVerbs(t *testing.T) {
	for _, raw := range []string{
		`{"type":"terminal.input","id":"1","terminalId":"t1","data":""}`,
		`{"type":"terminal.resize","id":"1","terminalId":"t1","cols":1000,"rows":1}`,
		`{"type":"worktree.create","id":"1","requestId":"r","project":"/p","name":"n","setup":false}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"working"},"context":0}`,
		`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"base","rev":"origin/main"}}`,
		`{"type":"git.commit.create","id":"1","tree":"/p","message":"","amend":true}`,
		`{"type":"gh.status","id":"1"}`,
		`{"type":"fs.search","id":"1","tree":"/p","query":""}`,
		`{"type":"config.set","id":"1","key":"phone.terminals","value":"on"}`,
		`{"type":"agent.timeline","id":"1","agentId":"a","beforeSeq":1}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != nil {
			t.Errorf("%s: %v", raw, err)
		}
	}
}

func TestDecodedPhoneFieldsHoldWhatWasSent(t *testing.T) {
	m, err := DecodeClient([]byte(`{"type":"git.diff","id":"1","tree":"/p","path":"a","at":{"kind":"commit","rev":"abc"},"context":0}`))
	if err != nil || m.At == nil || *m.At != (DiffAt{"commit", "abc"}) || m.Context == nil || *m.Context != 0 {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient([]byte(`{"type":"terminal.input","id":"1","terminalId":"t1","data":"bHMNCg=="}`))
	if err != nil || string(m.Data) != "ls\r\n" {
		t.Fatalf("%q %v", m.Data, err)
	}
	m, err = DecodeClient([]byte(`{"type":"git.status","id":"1","tree":"/p","limit":7}`))
	if err != nil || m.Limit == nil || *m.Limit != 7 {
		t.Fatalf("%+v %v", m.Limit, err)
	}
	m, err = DecodeClient([]byte(`{"type":"worktree.create","id":"1","requestId":"r","project":"/p","name":"","pr":118}`))
	if err != nil || m.Number != 118 || m.Setup != nil {
		t.Fatalf("%+v %v", m, err)
	}
}

func TestATerminalFrameIsKindIdLengthIdThenPayload(t *testing.T) {
	got, err := AppendFrame(nil, FrameOutput, "t12", []byte("hi\x1b[0m"))
	want := append([]byte{2, 3, 't', '1', '2'}, "hi\x1b[0m"...)
	if err != nil || string(got) != string(want) {
		t.Fatalf("%q %v", got, err)
	}
	if got, _ := AppendFrame(nil, FrameSnapshot, "t1", nil); string(got) != "\x01\x02t1" {
		t.Fatalf("empty snapshot %q", got)
	}
	for _, bad := range []string{"", strings.Repeat("t", 256), "t\n1", "té"} {
		if _, err := AppendFrame(nil, FrameOutput, bad, nil); err == nil {
			t.Errorf("%q framed", bad)
		}
	}
}

func TestAGitWatchHoldsAtMost32Trees(t *testing.T) {
	trees := func(n int) string {
		var out []string
		for i := range n {
			out = append(out, fmt.Sprintf(`"/p/%d"`, i))
		}
		return `{"type":"git.watch","id":"1","trees":[` + strings.Join(out, ",") + `]}`
	}
	if _, err := DecodeClient([]byte(trees(GitWatchLimit))); err != nil {
		t.Fatal(err)
	}
	if _, err := DecodeClient([]byte(trees(GitWatchLimit + 1))); err == nil {
		t.Fatal("33 trees decoded")
	}
}
