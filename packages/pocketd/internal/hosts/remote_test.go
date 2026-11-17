package hosts

import (
	"context"
	"encoding/json"
	"fmt"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"pocketd/internal/proto"
)

func TestAHostsSessionIsKeptUnderItsTagWithNoTerminalHere(t *testing.T) {
	got, ok := mirrored("devbox", proto.AgentSummary{ID: "a2", Parent: "a1", TerminalID: "t9", Cwd: "/srv/app", Status: "working"})
	if !ok || got.ID != "devbox/a2" || got.Parent != "devbox/a1" || got.TerminalID != "" || got.Cwd != "/srv/app" {
		t.Fatalf("mirrored = %+v, %v", got, ok)
	}
	if got, _ := mirrored("devbox", proto.AgentSummary{ID: "a3"}); got.Parent != "" {
		t.Errorf("a session without a parent got %q", got.Parent)
	}
}

func TestAnIDFromAHostThatCouldClimbOutOfAFolderIsRefused(t *testing.T) {
	for _, s := range []proto.AgentSummary{{ID: "../../etc"}, {ID: "a2", Parent: "a/../b"}, {ID: ""}} {
		if got, ok := mirrored("devbox", s); ok {
			t.Errorf("%+v was kept as %+v", s, got)
		}
	}
}

type asks struct {
	Mirror
	asked, answered []string
}

func (a *asks) Summary(proto.AgentSummary) {}
func (a *asks) Asked(req proto.PermissionRequest) {
	a.asked = append(a.asked, req.AgentID+" "+req.RequestID)
}
func (a *asks) Answered(session, request string) {
	a.answered = append(a.answered, session+" "+request)
}

func TestAQuestionAskedBeforeTheLaunchReturnedReachesTheMirrorOnceTracked(t *testing.T) {
	m := &asks{}
	r := newRemote(&Manager{Mirror: m, summaries: map[string]proto.AgentSummary{}}, "devbox", nil)
	ask := func(agent, request string) {
		detail := proto.ToolDetail{Kind: "other", Name: "AskUserQuestion", Input: json.RawMessage(`{"questions":[{"question":"Which?"}]}`)}
		raw, err := json.Marshal(proto.NewPermissionRequest(proto.PermissionRequest{RequestID: request, AgentID: agent, ToolName: "AskUserQuestion", Detail: detail}))
		if err != nil {
			t.Fatal(err)
		}
		r.frame("permission.request", raw)
	}
	ask("a2", "q1")
	ask("a2", "q2")
	raw, _ := json.Marshal(proto.NewPermissionResolved("q2", "allow"))
	r.frame("permission.resolved", raw)
	ask("a3", "q3")
	if len(m.asked) != 0 {
		t.Fatalf("asked before tracking: %v", m.asked)
	}
	r.track("a2", proto.AgentSummary{ID: "a2", Provider: "claude"})
	if !slices.Equal(m.asked, []string{"devbox/a2 devbox/q1"}) {
		t.Fatalf("asked = %v, want only a2's open question", m.asked)
	}
	raw, _ = json.Marshal(proto.NewPermissionResolved("q1", "allow"))
	r.frame("permission.resolved", raw)
	if !slices.Equal(m.answered, []string{"devbox/a2 devbox/q1"}) {
		t.Errorf("answered = %v", m.answered)
	}
}

type kept struct {
	asks
	open []string
}

func (k *kept) Live(string) []proto.AgentSummary {
	return []proto.AgentSummary{{ID: "devbox/a1", Status: "needsYou"}}
}
func (k *kept) Open(string) []string { return k.open }

// reconnected is a remote to a host whose hello listed a1 asking, as it was mirrored before, then sent asks again.
func reconnected(t *testing.T, m Mirror, asks ...string) *remote {
	t.Helper()
	r := newRemote(&Manager{Mirror: m, summaries: map[string]proto.AgentSummary{}}, "devbox", nil)
	raw, _ := json.Marshal(proto.AgentList{Type: "agent.list", Agents: []proto.AgentSummary{{ID: "a1", Status: "needsYou"}}})
	r.frame("agent.list", raw)
	for _, id := range asks {
		detail := proto.ToolDetail{Kind: "other", Name: "AskUserQuestion", Input: json.RawMessage(`{}`)}
		raw, err := json.Marshal(proto.NewPermissionRequest(proto.PermissionRequest{RequestID: id, AgentID: "a1", ToolName: "AskUserQuestion", Detail: detail}))
		if err != nil {
			t.Fatal(err)
		}
		r.frame("permission.request", raw)
	}
	return r
}

func TestAnAskStillOpenHereIsNotAskedAgainWhenTheLinkComesBack(t *testing.T) {
	m := &kept{open: []string{"devbox/q1"}}
	reconnected(t, m, "q1", "q2")
	if !slices.Equal(m.asked, []string{"devbox/a1 devbox/q2"}) {
		t.Fatalf("asked = %v", m.asked)
	}
}

type seqs struct {
	Mirror
	got []int64
}

func (m *seqs) Item(_ string, _ int64, it proto.Item) { m.got = append(m.got, it.Seq) }

func TestASessionMoreThanAPageAheadIsMirroredFromItsFirstUnseenItem(t *testing.T) {
	var all []proto.Item
	for seq := int64(1); seq <= 1234; seq++ {
		all = append(all, proto.Item{ID: fmt.Sprint("i", seq), Seq: seq, Kind: "assistant", Text: "x"})
	}
	// Answers as a host's Timeline does: the newest limit items past sinceSeq, or before beforeSeq.
	socket := fakePocketd(t, func(read func() map[string]any, send func(any)) {
		for m := read(); m != nil; m = read() {
			since, _ := m["sinceSeq"].(float64)
			before, backwards := m["beforeSeq"].(float64)
			var match []proto.Item
			for _, it := range all {
				if backwards && float64(it.Seq) < before || !backwards && float64(it.Seq) > since {
					match = append(match, it)
				}
			}
			page := match[len(match)-min(len(match), int(m["limit"].(float64))):]
			send(proto.NewAgentTimeline(m["id"].(string), "a1", 1, page, len(page) < len(match) || !backwards && since > 0, 1234))
		}
	})
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	c, err := Dial(ctx, socket, "", func(string, []byte) {})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	m := &seqs{}
	r := newRemote(&Manager{Mirror: m, summaries: map[string]proto.AgentSummary{}}, "devbox", nil)
	r.client = c
	r.seq["a1"] = 100
	r.catchUp("a1", false)
	if len(m.got) != 1134 || m.got[0] != 101 || !slices.IsSorted(m.got) || m.got[len(m.got)-1] != 1234 {
		t.Fatalf("mirrored %d items, from %v", len(m.got), m.got[:min(len(m.got), 3)])
	}
}

type statuses struct {
	Mirror
	live []proto.AgentSummary
	got  []string
}

func (m *statuses) Summary(s proto.AgentSummary)     { m.got = append(m.got, s.Status) }
func (m *statuses) Item(string, int64, proto.Item)   {}
func (m *statuses) Live(string) []proto.AgentSummary { return m.live }
func (m *statuses) Open(string) []string             { return nil }

// replying is a host whose session a1 replied and ended its turn without a result item.
func replying(t *testing.T, m Mirror) *remote {
	t.Helper()
	items := []proto.Item{{ID: "u", Seq: 1, Kind: "user", Text: "go"}, {ID: "a", Seq: 2, Kind: "assistant", Text: "done it"}}
	socket := fakePocketd(t, func(read func() map[string]any, send func(any)) {
		for msg := read(); msg != nil; msg = read() {
			var page []proto.Item
			if since, _ := msg["sinceSeq"].(float64); since < 2 {
				page = items[int(since):]
			}
			send(proto.NewAgentTimeline(msg["id"].(string), "a1", 1, page, false, 2))
		}
	})
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	c, err := Dial(ctx, socket, "", func(string, []byte) {})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { c.Close() })
	r := newRemote(&Manager{Mirror: m, summaries: map[string]proto.AgentSummary{}}, "devbox", nil)
	r.client = c
	return r
}

func TestASessionWhoseTurnEndedWithoutAResultBeforeItWasTrackedSettles(t *testing.T) {
	m := &statuses{}
	r := replying(t, m)
	r.latest["a1"] = proto.AgentSummary{ID: "a1", Status: "done"}
	r.track("a1", proto.AgentSummary{ID: "a1"})
	if !slices.Equal(m.got, []string{"working", "done"}) {
		t.Fatalf("mirrored %v", m.got)
	}
}

func TestASessionMirroredMidTurnThatIsIdleOnReconnectWithoutAResultSettles(t *testing.T) {
	m := &statuses{live: []proto.AgentSummary{{ID: "devbox/a1", Status: "working"}}}
	r := replying(t, m)
	raw, _ := json.Marshal(proto.AgentList{Type: "agent.list", Agents: []proto.AgentSummary{{ID: "a1", Status: "idle"}}})
	r.frame("agent.list", raw)
	if !slices.Equal(m.got, []string{"idle"}) {
		t.Fatalf("mirrored %v", m.got)
	}
}

type stored struct {
	Mirror
	mu       sync.Mutex
	sessions map[string]proto.AgentSummary
	items    int
	fed      chan string
}

func (m *stored) Summary(s proto.AgentSummary) {
	m.mu.Lock()
	m.sessions[s.ID] = s
	m.mu.Unlock()
	m.fed <- s.Status
}

func (m *stored) Item(string, int64, proto.Item) {
	m.mu.Lock()
	m.items++
	m.mu.Unlock()
}

func (m *stored) Live(string) []proto.AgentSummary {
	m.mu.Lock()
	defer m.mu.Unlock()
	return slices.Collect(maps.Values(m.sessions))
}

func (m *stored) Open(string) []string { return nil }

func TestAReconnectedLinkReadsOnlyTheItemsPastThoseMirroredBefore(t *testing.T) {
	var all []proto.Item
	for seq := int64(1); seq < 1234; seq++ {
		all = append(all, proto.Item{ID: fmt.Sprint("i", seq), Seq: seq, Kind: "assistant", Text: "x"})
	}
	all = append(all, proto.Item{ID: "end", Seq: 1234, Kind: "result", OK: true})
	var conns atomic.Int32
	var requests [3]atomic.Int32
	socket := fakePocketd(t, func(read func() map[string]any, send func(any)) {
		conn := conns.Add(1)
		for m := read(); m != nil; m = read() {
			requests[conn].Add(1)
			since, _ := m["sinceSeq"].(float64)
			before, backwards := m["beforeSeq"].(float64)
			var match []proto.Item
			for _, it := range all {
				if backwards && float64(it.Seq) < before || !backwards && float64(it.Seq) > since {
					match = append(match, it)
				}
			}
			page := match[len(match)-min(len(match), int(m["limit"].(float64))):]
			send(proto.NewAgentTimeline(m["id"].(string), "a1", 1, page, len(page) < len(match), 1234))
		}
	}, proto.AgentSummary{ID: "a1", Status: "done"})
	home, err := os.MkdirTemp("/tmp", "hm")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(home) })
	m, err := Open(Paths{Home: home, RemoteDir: ".cp"}, os.Environ, "1.2.0", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := m.Add(alias("devbox"), ""); err != nil {
		t.Fatal(err)
	}
	os.MkdirAll(filepath.Join(home, "hosts"), 0o700)
	if err := os.Symlink(socket, m.paths.Socket("devbox")); err != nil {
		t.Fatal(err)
	}
	mirror := &stored{sessions: map[string]proto.AgentSummary{"devbox/a1": {ID: "devbox/a1", Status: "working"}}, fed: make(chan string, 8)}
	m.Mirror, m.ctx = mirror, context.Background()
	link := func(l Link) {
		m.mu.Lock()
		m.follow("devbox", l)
		m.mu.Unlock()
	}
	fed := func() {
		t.Helper()
		select {
		case s := <-mirror.fed:
			if s != "done" {
				t.Fatalf("mirrored %s", s)
			}
		case <-time.After(10 * time.Second):
			t.Fatal("nothing mirrored")
		}
	}

	link(Link{State: StateOnline})
	fed()
	link(Link{State: StateRetrying})
	link(Link{State: StateOnline})
	fed()

	mirror.mu.Lock()
	defer mirror.mu.Unlock()
	if got := requests[2].Load(); got != 1 || mirror.items != len(all) {
		t.Fatalf("after the reconnect: %d page requests, %d items mirrored in all", got, mirror.items)
	}
}
