package factory

import (
	"bytes"
	"cmp"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/team"
)

// dir is where a Ticket's spec and activity are kept.
func dir(home, ticket string) string {
	return filepath.Join(home, "state", "factory", ticket)
}

func appendEvent(home, ticket string, ev proto.FactoryEvent) error {
	line, err := json.Marshal(ev)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(dir(home, ticket), 0o700); err != nil {
		return err
	}
	f, err := os.OpenFile(filepath.Join(dir(home, ticket), "activity.jsonl"), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = f.Write(append(line, '\n'))
	return err
}

// ReadActivity is a Ticket's events, oldest first. A Ticket with no file has none.
func ReadActivity(home, ticket string) ([]proto.FactoryEvent, error) {
	raw, err := os.ReadFile(filepath.Join(dir(home, ticket), "activity.jsonl"))
	if errors.Is(err, fs.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	var out []proto.FactoryEvent
	for _, line := range bytes.Split(raw, []byte{'\n'}) {
		var ev proto.FactoryEvent
		// A crash can cut the last line short.
		if len(line) == 0 || json.Unmarshal(line, &ev) != nil {
			continue
		}
		out = append(out, ev)
	}
	return out, nil
}

// Notes is what one change of a Ticket adds to its activity. team is the lead's Team, if it has one.
func Notes(before, after proto.Ticket, team *proto.Team, at int64) []proto.FactoryEvent {
	var out []proto.FactoryEvent
	add := func(station, actor, kind, text string) {
		out = append(out, proto.FactoryEvent{At: at, Station: station, Actor: actor, Kind: kind, Text: text})
	}
	gate := before.Gate == proto.GatePlan
	fails := after.Status == proto.TicketFailed
	switch {
	case gate && after.Station == proto.StationImplement:
		add(proto.StationPlan, "you", "approved", workers(team))
	case gate && after.Station == proto.StationPlan && after.Gate == "" && !fails:
		add(proto.StationPlan, "you", "changes", "")
	case before.Gate == proto.GatePR && after.Station == proto.StationImplement:
		add(proto.StationPR, "you", "changes", "")
	}
	if after.Station != before.Station {
		add(after.Station, "anywhere", "moved", "")
	}
	if fails && (before.Status != proto.TicketFailed || before.Why != after.Why) {
		add(after.Station, "anywhere", "failed", after.Why)
	}
	if after.Gate != before.Gate && (after.Gate == proto.GatePlan || after.Gate == proto.GatePR) {
		add(after.Station, "anywhere", "gate", "")
	}
	return out
}

func workers(team *proto.Team) string {
	if team == nil {
		return ""
	}
	if n := len(team.Tasks); n != 1 {
		return fmt.Sprintf("%d workers", n)
	}
	return "1 worker"
}

// note appends ev to the Ticket's activity, then tells the factory.update feed.
func (e *Engine) note(ticket string, ev proto.FactoryEvent) {
	if err := appendEvent(e.Home, ticket, ev); err != nil {
		log.Printf("factory: ticket %s: %v", ticket, err)
	}
	e.tell(ticket)
}

// session names agentID in t's activity: the lead, or the worker of a task.
func (e *Engine) session(agentID string, t proto.Ticket) string {
	teams, _ := e.Teams.Store.List()
	if _, task, ok := team.Find(teams, agentID); ok && agentID != t.AgentID {
		return "worker:" + task.ID
	}
	return "lead"
}

// said notes one thing about a request of a Factory session. A request of any other session is not noted.
func (e *Engine) said(req proto.PermissionRequest, kind string, you bool) {
	if e == nil {
		return
	}
	t, ok := e.owner(req.AgentID)
	if !ok {
		return
	}
	actor := "you"
	if !you {
		actor = e.session(req.AgentID, t)
	}
	e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: t.Station, Actor: actor, Kind: kind, Text: cmp.Or(req.Detail.Command, req.ToolName)})
}

// AutoAllowed notes a command the team plan's allowlist let through.
func (e *Engine) AutoAllowed(req proto.PermissionRequest) { e.said(req, "autoAllowed", false) }

// Asked is broker.Broker.OnAsk's: an Ask reached the user.
func (e *Engine) Asked(req proto.PermissionRequest) { e.said(req, "asked", false) }

// Resolved is broker.Broker.OnResolve's: the user's answer to an Ask.
func (e *Engine) Resolved(req proto.PermissionRequest, decision string) {
	kind := "denied"
	if decision == "allow" {
		kind = "allowed"
	}
	e.said(req, kind, true)
}

// Handback is team.Teams.OnHandback's: a worker handed its task back.
func (e *Engine) Handback(lead string, task proto.TeamTask) {
	if e == nil {
		return
	}
	t, ok := e.led(lead)
	if !ok {
		return
	}
	first, _, _ := strings.Cut(strings.TrimSpace(task.Summary), "\n")
	e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: t.Station, Actor: "worker:" + task.ID, Kind: "handback", Text: first})
}
