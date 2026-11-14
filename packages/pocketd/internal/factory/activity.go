package factory

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log"
	"os"
	"path/filepath"

	"pocketd/internal/proto"
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
	}
	if after.Station != before.Station {
		add(after.Station, "anywhere", "moved", "")
	}
	if fails && (before.Status != proto.TicketFailed || before.Why != after.Why) {
		add(after.Station, "anywhere", "failed", after.Why)
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
