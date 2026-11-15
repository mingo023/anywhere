package factory

import (
	"path/filepath"
	"strconv"
	"strings"
	"unicode"

	"pocketd/internal/proto"
)

// Key is a Ticket's short id as the desktop writes it: the first three letters
// or digits of its project's name in upper case, a hyphen, its number. Empty
// when it has no number or its project's name has neither.
func Key(project string, number int64) string {
	var prefix []rune
	for _, r := range project {
		if len(prefix) < 3 && (unicode.IsLetter(r) || unicode.IsDigit(r)) {
			prefix = append(prefix, unicode.ToUpper(r))
		}
	}
	if number <= 0 || len(prefix) == 0 {
		return ""
	}
	return string(prefix) + "-" + strconv.FormatInt(number, 10)
}

// led is the live Factory Ticket whose lead is agentID.
func (e *Engine) led(agentID string) (proto.Ticket, bool) {
	tickets, _ := e.Board.Snapshot()
	for _, t := range tickets {
		if agentID != "" && t.AgentID == agentID && t.State == proto.TicketLive && t.Station != "" {
			return t, true
		}
	}
	return proto.Ticket{}, false
}

// label names lead's work for a list a person reads: its Ticket's key, or its title when it has no number.
func (e *Engine) label(lead string) string {
	t, ok := e.led(lead)
	if !ok {
		return "A team's task"
	}
	if key := Key(e.project(t.Folder), t.Number); key != "" {
		return key
	}
	return strings.TrimSpace(t.Title)
}

// Profile is team.Teams.Profiles: the agent and model profile id runs.
func (e *Engine) Profile(id string) (provider, model string, ok bool) {
	if e == nil || e.Profiles == nil {
		return "", "", false
	}
	p, ok := e.Profiles.Get(id)
	return p.Provider, p.Model, ok
}

// Workers is team.Teams.Workers: the profile a task of lead's team takes when its pick names none.
func (e *Engine) Workers(lead string) string {
	if e == nil {
		return ""
	}
	t, ok := e.led(lead)
	if !ok {
		return ""
	}
	return e.Configs.Get(t.Folder).Workers
}

// Skipped notes each of a profile's skills that the checkout of lead's task kept its own folder for.
func (e *Engine) Skipped(lead, task string, skills []string) {
	if e == nil {
		return
	}
	t, ok := e.led(lead)
	if !ok {
		return
	}
	for _, skill := range skills {
		e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: t.Station, Actor: "worker:" + task, Kind: "skill", Text: skippedSkill(skill)})
	}
}

// Title is a lead session's name: its Ticket's key, then its title.
func Title(key, title string) string {
	if key == "" {
		return title
	}
	return key + " · " + title
}

// project is the name of the project in folder, as the desktop shows it.
func (e *Engine) project(folder string) string {
	if e.Teams == nil || e.Teams.Projects == nil {
		return filepath.Base(folder)
	}
	return e.Teams.Projects.Load().Name(folder)
}

// rename names t's lead session for its Ticket, so a list of sessions reads as a list of Tickets.
func (e *Engine) rename(t proto.Ticket, lead string) {
	a, ok := e.lead(lead)
	if !ok {
		return
	}
	if claim, ok := a.ClaimNaming(); ok {
		a.SetNamed(claim, Title(Key(e.project(t.Folder), t.Number), t.Title))
	}
}
