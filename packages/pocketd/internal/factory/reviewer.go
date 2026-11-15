package factory

import (
	"errors"
	"log"
	"os"
	"strconv"

	"pocketd/internal/agent"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
)

// reviewer is the open session with id, or nil.
func (e *Engine) reviewer(id string) *agent.Agent {
	if id == "" {
		return nil
	}
	a, err := e.Agents.Get(id)
	if err != nil || a.Turn() == "closed" {
		return nil
	}
	return a
}

// prFacts fills what a Ticket at PR review is decided from. The lead may be gone: you can still merge.
func (e *Engine) prFacts(f *Facts, ev event) {
	f.Local = e.local(f.Ticket)
	f.PR = e.prs[f.Ticket.Worktree]
	f.Review, f.Reviewer = e.reviewFacts(f.Ticket, f.PR, ev)
}

// reviewFacts is how far the review of pr's head has got, and the reviewer's session. It settles a turn that
// ended: a valid block is saved as the findings, and a second bad reply fails the review. A turn that has
// not reached the timeline yet reads as working, so Next never repairs a reviewer that has not answered.
func (e *Engine) reviewFacts(t proto.Ticket, pr *proto.PR, ev event) (ReviewState, Session) {
	if pr == nil {
		return ReviewNone, Session{}
	}
	rec := readReview(e.Home, t.ID).of(pr.Head)
	a := e.reviewer(rec.Agent)
	if rec.State != ReviewRunning && rec.State != ReviewRepaired {
		return rec.State, Session{Present: a != nil}
	}
	if rec.Agent == "" {
		// launchReviewer records the agent once the session exists.
		return rec.State, Session{}
	}
	if a == nil {
		e.finishReview(t, rec, ReviewFailed, nil)
		return ReviewFailed, Session{}
	}
	if ev.agent != rec.Agent {
		ev = event{}
	}
	reply := e.read(a, rec.Since, ev)
	if !reply.Ended {
		return rec.State, Session{Present: true, Turn: "working"}
	}
	state, findings, _ := verdict(rec.State, reply.Text, reply.Failed)
	if state != rec.State {
		e.finishReview(t, rec, state, findings)
	}
	return state, Session{Present: true, Turn: "done"}
}

// finishReview saves how rec's review ended and notes it.
func (e *Engine) finishReview(t proto.Ticket, rec reviewRecord, state ReviewState, findings []proto.Finding) {
	rec.State, rec.Findings = state, findings
	if rec.Findings == nil {
		rec.Findings = []proto.Finding{}
	}
	if err := writeReview(e.Home, t.ID, rec); err != nil {
		log.Printf("factory: saving the review of %s: %v", t.ID, err)
	}
	text := tally(findings)
	if state == ReviewFailed {
		text = NoFindings
	}
	e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationPR, Actor: "reviewer", Kind: "finding", Text: text})
}

// startReview records that the review of pr's head began, then launches the reviewer off the engine's
// goroutine: a launch waits for the CLI to attach.
func (e *Engine) startReview(t proto.Ticket, cfg proto.FactoryConfig, pr *proto.PR) {
	// A reviewer of an older head may still be open.
	e.closeReviewer(t)
	rec := reviewRecord{Head: pr.Head, State: ReviewRunning, Findings: []proto.Finding{}}
	if err := writeReview(e.Home, t.ID, rec); err != nil {
		log.Printf("factory: starting the review of %s: %v", t.ID, err)
		return
	}
	if lead, ok := e.lead(t.AgentID); ok {
		lead.SetAwaiting(false)
	}
	go e.launchReviewer(t, cfg, pr.Base, rec)
}

func (e *Engine) launchReviewer(t proto.Ticket, cfg proto.FactoryConfig, base string, rec reviewRecord) {
	fail := func(why string) {
		rec.State = ReviewFailed
		writeReview(e.Home, t.ID, rec)
		e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationPR, Actor: "anywhere", Kind: "failed", Text: why})
		e.post(event{ticket: t.ID})
	}
	p, ok := e.Profiles.Get(cfg.Review)
	switch {
	case !ok:
		fail("The reviewer profile " + cfg.Review + " is gone")
		return
	case providers.NeverAsks(p.Provider):
		fail("The reviewer's agent can't be kept read-only")
		return
	case e.Launch == nil:
		fail("The reviewer didn't start: nothing to launch it")
		return
	}
	spec := proto.LaunchSpec{Project: t.Folder, Checkout: proto.Checkout{Worktree: t.Worktree}, Provider: p.Provider, Model: p.Model, Access: "ask", Chat: true, Profile: cfg.Review}
	if _, ok := e.lead(t.AgentID); ok {
		spec.Parent = t.AgentID
	}
	r := e.Launch("factory:"+t.ID, "review-"+t.ID+"-"+strconv.FormatInt(e.now().UnixNano(), 36), spec, func(string) {})
	if r.Err != nil {
		fail("The reviewer didn't start: " + r.Err.Message)
		return
	}
	a, err := e.Agents.Get(r.AgentID)
	if err != nil {
		fail("The reviewer didn't start: " + err.Error())
		return
	}
	// Before anything else, so none of its asks reaches you before Deny knows whose they are.
	e.setReviewer(r.AgentID, t.ID)
	if now := readReview(e.Home, t.ID); now.Head != rec.Head || now.State != ReviewRunning {
		// The lead took the Ticket back to Implement meanwhile.
		e.dropReviewer(r.AgentID)
		e.closeLead(r.AgentID)
		return
	}
	if claim, ok := a.ClaimNaming(); ok {
		a.SetNamed(claim, "Review · "+t.Title)
	}
	rec.Agent = r.AgentID
	_, rec.Since = a.Timeline.State()
	if err := writeReview(e.Home, t.ID, rec); err != nil {
		fail("The review couldn't be saved: " + err.Error())
		return
	}
	if err := prompt(a, ReviewRequest(t, base, readSpec(e.Home, t.ID))); err != nil {
		fail("The reviewer didn't take its prompt: " + err.Error())
		return
	}
	e.post(event{ticket: t.ID})
}

func prompt(a *agent.Agent, text string) error {
	d := a.Driver()
	if d == nil {
		return errors.New("its session is not a chat")
	}
	return d.Prompt(text)
}

// repairReview sends the one repair prompt to a reviewer whose reply held no block that could be used.
func (e *Engine) repairReview(t proto.Ticket, pr *proto.PR) {
	rec := readReview(e.Home, t.ID).of(pr.Head)
	a := e.reviewer(rec.Agent)
	if rec.State != ReviewRunning || a == nil {
		return
	}
	_, err := ParseReview(e.read(a, rec.Since, event{}).Text)
	if err == nil {
		return
	}
	rec.State = ReviewRepaired
	_, rec.Since = a.Timeline.State()
	if writeReview(e.Home, t.ID, rec) != nil || prompt(a, RepairReview(err)) != nil {
		e.finishReview(t, rec, ReviewFailed, nil)
	}
}

// closeReviewer closes t's reviewer. A review that it had not finished is forgotten, so the next head, or
// the same one, is reviewed from the start.
func (e *Engine) closeReviewer(t proto.Ticket) {
	rec := readReview(e.Home, t.ID)
	e.closeLead(rec.Agent)
	e.dropReviewer(rec.Agent)
	if rec.State == ReviewRunning || rec.State == ReviewRepaired {
		os.Remove(reviewPath(e.Home, t.ID))
	}
}

// recoverReview gives Deny its reviewer back after a restart, and forgets a review whose launch the restart cut.
func (e *Engine) recoverReview(t proto.Ticket) {
	rec := readReview(e.Home, t.ID)
	running := rec.State == ReviewRunning || rec.State == ReviewRepaired
	switch {
	case e.reviewer(rec.Agent) != nil:
		e.setReviewer(rec.Agent, t.ID)
	case running && rec.Agent == "":
		os.Remove(reviewPath(e.Home, t.ID))
	}
}
