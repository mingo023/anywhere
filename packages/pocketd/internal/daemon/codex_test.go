package daemon

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
	"time"

	"pocketd/internal/codex"
	"pocketd/internal/codex/codextest"
	"pocketd/internal/terminal"
	"pocketd/internal/timeline"
)

const codexHistory = `{"thread":{"name":"Codex task","turns":[{"id":"t1","status":"completed","items":[
 {"type":"userMessage","id":"u1","content":[{"type":"text","text":"first"}]},
 {"type":"agentMessage","id":"a1","text":"hi"}]}]}}`

// codexHome starts an app-server for a new CODEX_HOME. Every thread resumes
// with codexHistory.
func codexHome(t *testing.T) (string, *codextest.Server) {
	home, _ := os.MkdirTemp("/tmp", "cx")
	t.Cleanup(func() { os.RemoveAll(home) })
	srv := codextest.Start(t, codex.Sock([]string{"CODEX_HOME=" + home}), func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return json.RawMessage(codexHistory), ""
		}
		return map[string]any{}, ""
	})
	return home, srv
}

func codexIn(t *testing.T, d *Daemon, home string, args ...string) (*terminal.Terminal, *presence) {
	t.Helper()
	term := shell(t, d)
	term.Write([]byte(strings.Join(append([]string{"CODEX_HOME=" + home, fakeAgent(t, "codex")}, args...), " ") + "\r"))
	waitAgent(t, d, term)
	return term, d.presentIn(term.Info().ID)
}

func active(id string) string {
	return `{"threadId":"` + id + `","status":{"type":"active","activeFlags":[]}}`
}

func boundTo(pr *presence) string {
	pr.mu.Lock()
	defer pr.mu.Unlock()
	return pr.thread
}

func TestACodexThreadBindsToTheCodexThatGotEnter(t *testing.T) {
	d := newDaemon(t)
	home, srv := codexHome(t)
	term, pr := codexIn(t, d, home)
	term.Write([]byte("\r"))
	srv.Broadcast("thread/status/changed", active("th1"))
	if got := string(srv.Next("thread/resume").Params); got != `{"threadId":"th1"}` {
		t.Fatalf("resume %s", got)
	}
	eventually(t, "the thread's history", func() bool {
		a := pr.a.Summary()
		return a.ProviderSessionID == "th1" && a.Title == "Codex task" && a.MaxSeq > 0 && a.Attached
	})
}

func TestACodexStartedOnAPromptBindsItsFirstThreadWithoutEnter(t *testing.T) {
	d := newDaemon(t)
	home, srv := codexHome(t)
	_, pr := codexIn(t, d, home, "--", "'do it'")
	pr.mu.Lock()
	pr.enter = time.Now().Add(-CodexMapWindow - time.Second)
	pr.mu.Unlock()
	srv.Broadcast("thread/status/changed", active("th1"))
	eventually(t, "bound", func() bool { return boundTo(pr) == "th1" })
	srv.Broadcast("thread/status/changed", `{"threadId":"th1","status":{"type":"idle"}}`)
	eventually(t, "the turn's end", func() bool { return pr.a.Summary().Status == "done" })
}

func TestACodexThreadNobodyTypedIsIgnored(t *testing.T) {
	d := newDaemon(t)
	home, _ := codexHome(t)
	term, pr := codexIn(t, d, home)
	w := &codexSock{d: d, sock: pr.sock, nonRoot: map[string]bool{}}
	elsewhere := &codexSock{d: d, sock: codex.Sock([]string{"CODEX_HOME=/elsewhere"}), nonRoot: map[string]bool{}}
	w.ThreadStatus("other", "active", nil)
	term.Write([]byte("\r"))
	elsewhere.ThreadStatus("away", "active", nil)
	w.ThreadStatus("idle", "idle", nil)
	w.ThreadStarted("sub", false)
	w.ThreadStatus("sub", "active", nil)
	pr.mu.Lock()
	pr.enter = time.Now().Add(-CodexMapWindow - time.Second)
	pr.mu.Unlock()
	w.ThreadStatus("late", "active", nil)
	if th := boundTo(pr); th != "" {
		t.Fatalf("bound %s", th)
	}
	term.Write([]byte("\r"))
	w.ThreadStatus("th1", "active", nil)
	w.ThreadStatus("th1", "idle", nil)
	w.ThreadStatus("th2", "active", nil)
	if th := boundTo(pr); th != "th1" {
		t.Fatalf("bound %q", th)
	}
}

func TestThreadClosedUnbindsItButTheAgentStays(t *testing.T) {
	d := newDaemon(t)
	home, srv := codexHome(t)
	term, pr := codexIn(t, d, home)
	term.Write([]byte("\r"))
	srv.Broadcast("thread/status/changed", active("th1"))
	eventually(t, "bound", func() bool { return boundTo(pr) == "th1" })
	srv.Broadcast("thread/closed", `{"threadId":"th1"}`)
	eventually(t, "unbound", func() bool { return boundTo(pr) == "" })
	eventually(t, "the follower gone", func() bool { return srv.Conns() == 1 })
	if _, ok := agentIn(d, term); !ok {
		t.Fatal("the agent left with its thread")
	}
}

func TestAThreadBoundAgainReplaysOnce(t *testing.T) {
	d := newDaemon(t)
	home, srv := codexHome(t)
	term, pr := codexIn(t, d, home)
	for _, seq := range []int64{3, 6} {
		term.Write([]byte("\r"))
		srv.Broadcast("thread/status/changed", active("th1"))
		eventually(t, "the thread's history", func() bool { return pr.a.Summary().MaxSeq == seq })
		srv.Broadcast("thread/closed", `{"threadId":"th1"}`)
		eventually(t, "unbound", func() bool { return boundTo(pr) == "" })
	}
	if items, _ := pr.a.Timeline.Page(0, 100); len(items) != 3 {
		t.Fatalf("%d items", len(items))
	}
}

func TestTheOldThreadStaysOutOfTheNewTimeline(t *testing.T) {
	d := newDaemon(t)
	home, _ := os.MkdirTemp("/tmp", "cx")
	t.Cleanup(func() { os.RemoveAll(home) })
	long := `{"thread":{"turns":[{"id":"t1","status":"inProgress","items":[` +
		strings.Repeat(`{"type":"userMessage","id":"u","content":[{"type":"text","text":"old"}]},`, 20000) +
		`{"type":"userMessage","id":"u","content":[]}]}]}}`
	codextest.Start(t, codex.Sock([]string{"CODEX_HOME=" + home}), func(method string, params json.RawMessage) (any, string) {
		switch {
		case method != "thread/resume":
			return map[string]any{}, ""
		case string(params) == `{"threadId":"th1"}`:
			return json.RawMessage(long), ""
		}
		return json.RawMessage(codexHistory), ""
	})
	term, pr := codexIn(t, d, home)
	w := &codexSock{d: d, sock: pr.sock, nonRoot: map[string]bool{}}
	term.Write([]byte("\r"))
	w.ThreadStatus("th1", "active", nil)
	eventually(t, "th1's history", func() bool { return pr.a.Summary().MaxSeq > 0 })
	w.ThreadStatus("th1", "idle", nil)
	term.Write([]byte("\r"))
	w.ThreadStatus("th2", "active", nil)
	eventually(t, "th2's history", func() bool {
		items, _ := pr.a.Timeline.Page(0, 1)
		return len(items) == 1 && items[0].Kind == "result"
	})
	if items, more := pr.a.Timeline.Page(0, 3); more || items[0].Text != "first" {
		t.Fatal("th1 landed in th2's timeline")
	}
}

func TestCodexOnOneAccountShareAWatcher(t *testing.T) {
	d := newDaemon(t)
	home, srv := codexHome(t)
	watchers := func() int {
		d.mu.Lock()
		defer d.mu.Unlock()
		return len(d.watchers)
	}
	a, _ := codexIn(t, d, home)
	b, _ := codexIn(t, d, home)
	if n := watchers(); n != 1 {
		t.Fatalf("%d watchers", n)
	}
	a.Close()
	<-a.Done()
	d.poll()
	if n := watchers(); n != 1 {
		t.Fatalf("%d watchers with one codex left", n)
	}
	b.Close()
	<-b.Done()
	d.poll()
	if n := watchers(); n != 0 {
		t.Fatalf("%d watchers with no codex left", n)
	}
	eventually(t, "the watcher's connection closed", func() bool { return srv.Conns() == 0 })
}

func TestEndingACodexStopsItsFollower(t *testing.T) {
	d := newDaemon(t)
	home, srv := codexHome(t)
	term, _ := codexIn(t, d, home)
	term.Write([]byte("\r"))
	srv.Broadcast("thread/status/changed", active("th1"))
	srv.Next("thread/resume")
	term.Close()
	<-term.Done()
	d.poll()
	eventually(t, "no connections", func() bool { return srv.Conns() == 0 })
}

func TestTheThreadStatusDrivesItsCodex(t *testing.T) {
	d := newDaemon(t)
	home, _ := codexHome(t)
	term, pr := codexIn(t, d, home)
	w := &codexSock{d: d, sock: pr.sock, nonRoot: map[string]bool{}}
	term.Write([]byte("\r"))
	for _, c := range []struct {
		typ    string
		flags  []string
		want   string
		failed bool
	}{
		{"active", nil, "working", false},
		{"active", []string{"waitingOnApproval"}, "needsYou", false},
		{"active", nil, "working", false},
		{"active", []string{"waitingOnUserInput"}, "needsYou", false},
		{"idle", nil, "done", false},
		{"notLoaded", nil, "done", false},
		{"active", nil, "working", false},
		{"systemError", nil, "done", true},
	} {
		w.ThreadStatus("th1", c.typ, c.flags)
		if a := pr.a.Summary(); a.Status != c.want || a.Failed != c.failed {
			t.Fatalf("%s %v: %s failed=%v", c.typ, c.flags, a.Status, a.Failed)
		}
	}
	w.ThreadClosed("th1")
	pr.mu.Lock()
	pr.enter = time.Time{}
	pr.mu.Unlock()
	w.ThreadStatus("th1", "active", nil)
	if s := pr.a.Summary().Status; s != "done" {
		t.Fatalf("a closed thread set %s", s)
	}
}

func TestABoundThreadStaysWithItsCodex(t *testing.T) {
	d := newDaemon(t)
	home, _ := codexHome(t)
	ta, a := codexIn(t, d, home)
	tb, b := codexIn(t, d, home)
	w := &codexSock{d: d, sock: a.sock, nonRoot: map[string]bool{}}
	ta.Write([]byte("\r"))
	w.ThreadStatus("th1", "active", nil)
	w.ThreadStatus("th1", "idle", nil)
	tb.Write([]byte("\r"))
	w.ThreadStatus("th1", "active", nil)
	if boundTo(a) != "th1" || boundTo(b) != "" || a.a.Summary().Status != "working" {
		t.Fatalf("a has %q (%s), b has %q", boundTo(a), a.a.Summary().Status, boundTo(b))
	}
}

func TestTwoCodexInOneFolderGetTheirOwnThreads(t *testing.T) {
	d := newDaemon(t)
	home, _ := codexHome(t)
	ta, a := codexIn(t, d, home)
	tb, b := codexIn(t, d, home)
	w := &codexSock{d: d, sock: a.sock, nonRoot: map[string]bool{}}
	tb.Write([]byte("\r"))
	ta.Write([]byte("\r"))
	w.ThreadStatus("th1", "active", nil)
	w.ThreadStatus("th2", "active", nil)
	if boundTo(a) != "th1" || boundTo(b) != "th2" {
		t.Fatalf("a has %q, b has %q", boundTo(a), boundTo(b))
	}
}

func TestACompactionItemEndsTheCompaction(t *testing.T) {
	a := newDaemon(t).Agents.Add("a1", "/w", "codex", nil)
	a.SetCompacting()
	codexSink{a}.Apply(timeline.Event{Kind: "compacted", Trigger: "manual"})
	if s := a.Summary(); s.Compacting || s.Status != "done" {
		t.Fatalf("compacting=%v %s", s.Compacting, s.Status)
	}
}

func TestEmbedded(t *testing.T) {
	for _, c := range []struct {
		argv, env []string
		want      bool
	}{
		{[]string{"codex"}, nil, false},
		{[]string{"codex", "resume", "--last"}, nil, false},
		{[]string{"codex", "-m", "gpt-5", "exec"}, nil, false},
		{[]string{"codex"}, []string{"CODEX_HOME=/h"}, false},
		{[]string{"codex", "exec", "fix it"}, nil, true},
		{[]string{"codex", "e", "fix it"}, nil, true},
		{[]string{"codex", "--profile=work"}, nil, true},
		{[]string{"codex", "resume", "-c", "model=o3"}, nil, true},
		{[]string{"codex"}, []string{"CODEX_EXEC_SERVER_URL=ws://h"}, true},
	} {
		if got := embedded(c.argv, c.env); got != c.want {
			t.Errorf("embedded(%q, %q) = %v", c.argv, c.env, got)
		}
	}
	for _, flag := range strings.Fields("--no-daemon --oss -p --profile -c --config --enable --disable --search --strict-config --dangerously-bypass-hook-trust --remote") {
		if !embedded([]string{"codex", flag, "x"}, nil) {
			t.Errorf("%s is not embedded", flag)
		}
	}
}

func TestAnEmbeddedCodexIsNotAttached(t *testing.T) {
	d := newDaemon(t)
	home, _ := codexHome(t)
	term, pr := codexIn(t, d, home, "exec", "fix")
	w := &codexSock{d: d, sock: codex.Sock([]string{"CODEX_HOME=" + home}), nonRoot: map[string]bool{}}
	term.Write([]byte("\r"))
	w.ThreadStatus("th1", "active", nil)
	if a := pr.a.Summary(); a.Attached || a.Status != "idle" || boundTo(pr) != "" || len(d.watchers) != 0 {
		t.Fatalf("agent %+v, bound %q, %d watchers", a, boundTo(pr), len(d.watchers))
	}
}
