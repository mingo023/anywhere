package main

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"path/filepath"
	"strings"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/automation"
	"pocketd/internal/config"
	"pocketd/internal/hub"
	"pocketd/internal/mcp"
	"pocketd/internal/orch"
	"pocketd/internal/orchhost"
	"pocketd/internal/proto"
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
func openOrchestrator(home string, settings *config.Settings, reg *agent.Registry, h *hub.Hub) (*orchestrator, error) {
	store, err := orch.Open(home)
	if err != nil {
		return nil, err
	}
	o := &orchestrator{store: store, sink: orch.NewSink(store)}
	reg.Sink = o.sink
	reg.Lineage = store.Lineage
	port := settings.MCPPort()
	o.ln, err = net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", port))
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
	if err := o.engine.Recover(); err != nil {
		log.Printf("orch: recover: %v", err)
	}
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

func (o *orchestrator) handover() {
	o.engine.Drain(5 * time.Second)
	o.sink.Flush()
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
