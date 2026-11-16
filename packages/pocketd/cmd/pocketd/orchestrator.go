package main

import (
	"cmp"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/automation"
	"pocketd/internal/config"
	"pocketd/internal/handoff"
	"pocketd/internal/hosts"
	"pocketd/internal/hub"
	"pocketd/internal/launch"
	"pocketd/internal/mcp"
	"pocketd/internal/orch"
	"pocketd/internal/orchhost"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/wsserver"
)

type orchestrator struct {
	store  *orch.Store
	sink   *orch.Sink
	engine *orch.Engine
	host   *orchhost.Host
	tools  *mcp.Toolkit
	server *mcp.Server
	ln     net.Listener
}

// openOrchestrator sets the registry's sink, so it runs before any agent is added.
// inherited is the MCP listener's fd an upgrading pocketd handed down, or 0.
func openOrchestrator(home string, settings *config.Settings, reg *agent.Registry, h *hub.Hub, inherited int) (*orchestrator, error) {
	store, err := orch.Open(home)
	if err != nil {
		return nil, err
	}
	o := &orchestrator{store: store, sink: orch.NewSink(store)}
	o.sink.Mirrored = hosts.Tagged
	reg.Sink = o.sink
	reg.Lineage = store.Lineage
	reg.History = store.History
	port := settings.MCPPort()
	if inherited > 0 {
		if o.ln, err = handoff.Inherit(inherited); err != nil {
			log.Printf("mcp: handed-down listener: %v", err)
		}
	}
	if o.ln == nil {
		o.ln, err = net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", port))
	}
	if err != nil {
		// Agents still running hold the saved port, so it isn't swapped for a free one; their tools stay off, the rest of pocketd goes on.
		log.Printf("mcp: tools are off: %v", err)
	}
	if o.ln != nil && port == 0 {
		port = o.ln.Addr().(*net.TCPAddr).Port
		if err := settings.SetMCPPort(port); err != nil {
			log.Printf("mcp: saving port: %v", err)
		}
	}
	o.host = &orchhost.Host{Registry: reg, Tokens: mcp.NewTokens(store), StateDir: filepath.Join(home, "state")}
	if o.ln != nil {
		o.host.URL = fmt.Sprintf("http://127.0.0.1:%d/mcp", port)
		mcp.HookBase = fmt.Sprintf("http://127.0.0.1:%d", port)
	}
	e := orch.NewEngine(store, o.host)
	// Effects wait until run, after the restore: an agent one acts on may not be listed before.
	e.Pause()
	o.host.SetEngine(e)
	o.engine = e
	e.Agent = orchhost.AgentInfo
	e.Limits = func() orch.Limits {
		d, s, m := settings.Limits()
		return orch.Limits{Depth: d, PerSession: s, PerMac: m}
	}
	e.Live = func(id string) (orch.Session, bool) {
		a, err := reg.Get(id)
		if err != nil {
			return orch.Session{}, false
		}
		s := a.Summary()
		return orch.Session{ID: s.ID, Agent: s.Provider, Access: s.Access, Status: s.Status, Project: s.Project, Cwd: s.Cwd, Worktree: s.Worktree, Title: s.Title,
			Conversation: s.ProviderSessionID, Terminal: s.TerminalID, CreatedAt: s.CreatedAt}, true
	}
	e.OnChange = func(c orch.Change) {
		for _, t := range c.Delegations {
			h.Publish(proto.NewDelegationUpdate(t.ParentID, wsserver.WireDelegation(e, t)))
			if t.ChildID != "" {
				if a, err := reg.Get(t.ChildID); err == nil {
					a.SetDelegation(&proto.DelegationRef{TaskID: t.TaskID, Status: t.Status, WorkState: t.WorkState})
				}
			}
		}
		for _, s := range c.Queues {
			h.Publish(proto.NewQueueUpdate(s, wsserver.QueueEntries(e, s)))
		}
	}
	o.sink.OnTurn = e.OnTurn
	o.tools = mcp.NewToolkit(o.host)
	e.OnClosed = func(session string) {
		o.host.Closed(session)
		if a, err := reg.Get(session); err == nil {
			o.tools.Forget(a.Summary().TerminalID)
		}
	}
	o.server = &mcp.Server{Tokens: o.host.Tokens, Tools: o.tools, Version: versionString()}
	return o, nil
}

// ask records a broker ask: AskUserQuestion is a question, anything else a permission ask.
func (o *orchestrator) ask(req proto.PermissionRequest) {
	kind := "permission"
	if req.ToolName == "AskUserQuestion" {
		kind = "question"
	}
	o.engine.AskOpened(req.AgentID, req.RequestID, kind, req.Detail.Input)
}

func (o *orchestrator) asked(req proto.PermissionRequest) {
	o.engine.AskClosed(req.AgentID, req.RequestID)
}

func (o *orchestrator) run(ctx context.Context) {
	o.engine.FailOrphans()
	if err := o.engine.Recover(); err != nil {
		log.Printf("orch: recover: %v", err)
	}
	o.engine.Resume()
	if o.ln != nil {
		go func() {
			if err := o.server.Serve(ctx, o.ln); err != nil {
				log.Printf("mcp: %v", err)
			}
		}()
	}
	prune := time.NewTicker(time.Hour)
	defer prune.Stop()
	for {
		if err := o.store.Prune(time.Now()); err != nil {
			log.Printf("orch: prune: %v", err)
		}
		select {
		case <-ctx.Done():
			o.engine.Close()
			o.sink.Close()
			return
		case <-prune.C:
		}
	}
}

// saved keeps in the store what a restart brings back, as terminals.json
// was just written with it; the file stays for a downgraded pocketd to read.
func (o *orchestrator) saved(f state.File) {
	entries := map[string][]byte{}
	for _, e := range f.Terminals {
		if e.AgentID != "" {
			entries[e.AgentID], _ = json.Marshal(e)
		}
	}
	if err := o.store.SetRestorable(entries); err != nil {
		log.Printf("orch: saving the sessions to restore: %v", err)
	}
}

// restoring is what a cold start brings back: the store's sessions, unless
// terminals.json at path was written by a pocketd without the store.
func (o *orchestrator) restoring(path string) state.File {
	_, err := os.Stat(path)
	found := err == nil
	file, err := state.Load(path)
	if err != nil {
		log.Print(err)
		found = false
	}
	entries, err := o.store.Restorable()
	if err != nil {
		log.Printf("orch: sessions to restore: %v; restoring from %s", err, path)
		return file
	}
	var stored []state.Terminal
	for _, raw := range entries {
		var e state.Terminal
		if json.Unmarshal(raw, &e) == nil {
			stored = append(stored, e)
		}
	}
	f, fromFile := state.Restoring(file, found, stored)
	if fromFile {
		o.takeIn(f.Terminals)
	}
	return f
}

// takeIn makes a store session of each terminal entry that held one, with
// the conversation, transcript, access and launch it was saved with: what an
// upgrade from a pocketd without the store hands down or left on disk.
func (o *orchestrator) takeIn(entries []state.Terminal) {
	var sessions []orch.Session
	for _, e := range entries {
		if e.AgentID == "" {
			continue
		}
		l := state.Launch{}
		if e.Launch != nil {
			l = *e.Launch
		}
		sessions = append(sessions, orch.Session{ID: e.AgentID, Agent: e.Provider, Cwd: e.LaunchDir, Terminal: e.TerminalID, Conversation: e.ConversationID,
			Access: l.Access, Model: l.Model, Effort: l.Effort, Pinned: e.Pinned, Chat: e.Chat, Origin: e.Origin, Status: cmp.Or(e.Status, "idle"), Failed: e.Failed, CreatedAt: e.CreatedAt})
	}
	if err := o.store.Import(sessions); err != nil {
		log.Printf("orch: taking in the sessions saved: %v", err)
	}
	o.saved(state.File{Terminals: entries})
}

// openSessions reads the store's open sessions once the sink has written
// what it was given, so a session just closed isn't read as open.
func (o *orchestrator) openSessions() ([]orch.Session, error) {
	o.sink.Flush()
	return o.store.OpenSessions()
}

// closeAbsent closes each session the store keeps open that this start
// brought back neither restored nor handed down.
func (o *orchestrator) closeAbsent(reg *agent.Registry) {
	err := o.store.CloseAbsent(func(id string) bool {
		_, err := reg.Get(id)
		return err == nil
	})
	if err != nil {
		log.Printf("orch: closing the sessions left behind: %v", err)
	}
}

func (o *orchestrator) handover() {
	o.engine.Drain(5 * time.Second)
	o.sink.Flush()
}

// postBound is automation.Scheduler.Post: it sends an automation's prompt to the open session it is bound to.
func postBound(reg *agent.Registry, e *orch.Engine) func(string, proto.Automation) (launch.Result, automation.Watcher, bool) {
	return func(runID string, a proto.Automation) (launch.Result, automation.Watcher, bool) {
		bound, err := reg.Get(a.Bound)
		if err != nil || bound.Turn() == "closed" {
			return launch.Result{}, nil, false
		}
		_, after := bound.Timeline.State()
		if _, err := e.Dispatch(orch.NewCommand("automation:"+runID, orch.CmdMessageSend, a.Bound, orch.SendPayload{Text: a.Prompt, Origin: orch.OriginUser})); err != nil {
			return launch.Result{}, nil, false
		}
		return launch.Result{AgentID: a.Bound, TerminalID: bound.Summary().TerminalID}, automation.TurnWatcher(reg, after), true
	}
}

type schedules struct {
	store     *automation.Store
	scheduler *automation.Scheduler
}

func (s schedules) List() []proto.Automation {
	list, _ := s.store.Snapshot()
	return list
}

func (s schedules) Save(a proto.Automation) (proto.Automation, error) {
	return s.store.Save(a, time.Now())
}

func (s schedules) Delete(id string) error                 { return s.store.Delete(id) }
func (s schedules) RunNow(id string) (proto.Run, error)    { return s.scheduler.RunNow(id) }
func (s schedules) Get(id string) (proto.Automation, bool) { return s.store.Automation(id) }

func hooks(s mcp.Schedules) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			http.Error(w, "POST only", http.StatusMethodNotAllowed)
			return
		}
		id := strings.TrimPrefix(r.URL.Path, "/hooks/")
		a, ok := s.Get(id)
		if !ok || !a.Webhook {
			http.NotFound(w, r)
			return
		}
		body, _ := io.ReadAll(io.LimitReader(r.Body, 1<<20))
		if a.WebhookSecret != "" {
			mac := hmac.New(sha256.New, []byte(a.WebhookSecret))
			mac.Write(body)
			want := hex.EncodeToString(mac.Sum(nil))
			if !hmac.Equal([]byte(want), []byte(r.Header.Get("X-Anywhere-Signature"))) {
				http.Error(w, "bad signature", http.StatusUnauthorized)
				return
			}
		}
		run, err := s.RunNow(id)
		if err != nil {
			http.Error(w, err.Error(), http.StatusConflict)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusAccepted)
		fmt.Fprintf(w, `{"runId":%q}`, run.ID)
	})
}
