package factory

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"pocketd/internal/atomicfile"
	"pocketd/internal/proto"
	"pocketd/internal/team"
)

const (
	reviewOpen  = "<anywhere_factory_review>"
	reviewClose = "</anywhere_factory_review>"
	// maxFindings bounds what one review may report.
	maxFindings = 50
)

const (
	ActStartReview   = "startReview"
	ActRepairReview  = "repairReview"
	ActCloseReviewer = "closeReviewer"
)

var severities = []string{"critical", "minor", "nit"}

// ReviewState is how far the review of the PR's head got.
type ReviewState string

const (
	ReviewNone     ReviewState = "none"
	ReviewRunning  ReviewState = "running"
	ReviewRepaired ReviewState = "repaired"
	ReviewDone     ReviewState = "done"
	ReviewFailed   ReviewState = "failed"
)

// NoFindings is the note of a review that ended without a block that could be used.
const NoFindings = "The reviewer returned no findings block"

// ReviewRequest is the reviewer's first prompt. It ends with spec, or with the Ticket when its project writes no spec.
func ReviewRequest(t proto.Ticket, base, spec string) string {
	against := "Spec:\n" + spec
	if spec == "" {
		against = "Ticket: " + t.Title + "\n\n" + t.Prompt
	}
	return `<anywhere_factory_review_request>
Review the changes of this branch against ` + base + `. You can read and run read-only commands. Don't edit, write or delete anything.
Check the work against the spec below. Reply with a short note, then exactly one block:
<anywhere_factory_review>
{"findings":[{"severity":"critical","file":"path/from/repo/root","line":12,"message":"What is wrong and why it matters"}]}
</anywhere_factory_review>
- "severity" is critical, minor or nit. "file" and "line" are optional.
- An empty list means you found nothing.
</anywhere_factory_review_request>

` + against
}

// ParseReview reads the findings of the last <anywhere_factory_review> block of text. The block is one JSON
// object with the key "findings" and nothing else.
func ParseReview(text string) ([]proto.Finding, error) {
	end := strings.LastIndex(text, reviewClose)
	start := -1
	if end >= 0 {
		start = strings.LastIndex(text[:end], reviewOpen)
	}
	if start < 0 {
		return nil, errors.New("no " + reviewOpen + " block")
	}
	dec := json.NewDecoder(strings.NewReader(text[start+len(reviewOpen) : end]))
	dec.DisallowUnknownFields()
	var block struct {
		Findings *[]proto.Finding `json:"findings"`
	}
	if err := dec.Decode(&block); err != nil {
		return nil, fmt.Errorf("the block is not the JSON that was asked for (%v)", err)
	}
	if _, err := dec.Token(); err != io.EOF {
		return nil, errors.New("the block holds more than one JSON object")
	}
	if block.Findings == nil {
		return nil, errors.New(`the block has no "findings" list`)
	}
	findings := *block.Findings
	if len(findings) > maxFindings {
		return nil, fmt.Errorf("the block has %d findings, and the most is %d", len(findings), maxFindings)
	}
	for i := range findings {
		f := &findings[i]
		f.Message, f.File = strings.TrimSpace(f.Message), strings.TrimSpace(f.File)
		switch {
		case !slices.Contains(severities, f.Severity):
			return nil, fmt.Errorf("finding %d has the severity %q, which is not critical, minor or nit", i+1, f.Severity)
		case f.Message == "":
			return nil, fmt.Errorf("finding %d has no message", i+1)
		case f.Line < 0:
			return nil, fmt.Errorf("finding %d has a negative line", i+1)
		}
	}
	return findings, nil
}

// RepairReview is the one prompt that asks a reviewer for its block again.
func RepairReview(err error) string {
	return "Your review block can't be used: " + err.Error() + ". Send the corrected " + reviewOpen + " block only."
}

// reviewRecord is state/factory/<ticket>/review.json: the review of one head of the Ticket's PR.
type reviewRecord struct {
	// Head is the oid of the PR's head that the reviewer read.
	Head  string      `json:"head"`
	State ReviewState `json:"state"`
	// Agent is the reviewer's session. A restart finds it here.
	Agent string `json:"agent,omitempty"`
	// Since is the reviewer's timeline seq before the prompt that it must answer.
	Since    int64           `json:"since,omitempty"`
	Findings []proto.Finding `json:"findings"`
}

func reviewPath(home, ticket string) string {
	return filepath.Join(dir(home, ticket), "review.json")
}

// readReview is ticket's record. A missing or broken file is no review.
func readReview(home, ticket string) reviewRecord {
	var r reviewRecord
	raw, err := os.ReadFile(reviewPath(home, ticket))
	if err != nil || json.Unmarshal(raw, &r) != nil || r.State == "" {
		return reviewRecord{State: ReviewNone}
	}
	return r
}

func writeReview(home, ticket string, r reviewRecord) error {
	path := reviewPath(home, ticket)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	raw, err := json.MarshalIndent(r, "", "\t")
	if err != nil {
		return err
	}
	return atomicfile.Write(path, raw, 0o600)
}

// of is r as it reads for head: the review of another head is no review of this one.
func (r reviewRecord) of(head string) reviewRecord {
	if head == "" || r.Head != head {
		return reviewRecord{State: ReviewNone}
	}
	return r
}

// verdict is how a reviewer's ended turn settles a review in state: its findings when reply holds a valid
// block, a failure after the one repair or a failed turn, else state again, which asks for the repair.
func verdict(state ReviewState, reply string, failed bool) (ReviewState, []proto.Finding, error) {
	findings, err := ParseReview(reply)
	switch {
	case err == nil:
		return ReviewDone, findings, nil
	case failed || state == ReviewRepaired:
		return ReviewFailed, nil, err
	}
	return state, nil, err
}

// tally counts findings by severity for the Activity: "1 critical · 2 minor".
func tally(findings []proto.Finding) string {
	var parts []string
	for _, s := range severities {
		if n := len(slices.DeleteFunc(slices.Clone(findings), func(f proto.Finding) bool { return f.Severity != s })); n > 0 {
			parts = append(parts, strconv.Itoa(n)+" "+s)
		}
	}
	if len(parts) == 0 {
		return "No findings"
	}
	return strings.Join(parts, " · ")
}

// ReadOnlyReason is what a reviewer is told when pocketd refuses its ask.
const ReadOnlyReason = "The reviewer is read-only. Report what you found instead of changing it."

// readCommands are the commands a reviewer runs without an Ask.
var readCommands = []string{"git diff", "git log", "git show", "git status", "ls", "cat", "rg", "grep"}

// escapes are the options with which a command of readCommands writes a file or runs a program.
var escapes = []string{"--output", "--ext-diff", "--pre", "--hostname-bin"}

// ReadOnly is whether a reviewer may run command: one command of readCommands by team.Allowed's rule. The shell
// could build an escape option out of quotes, braces or a variable, so a command with a backslash, a dollar sign
// or a brace is refused, and quotes are dropped before each word is compared. git takes any unambiguous start of
// a long option, so a word that starts an escape option is refused too.
func ReadOnly(command string) bool {
	if strings.ContainsAny(command, `\${}`) || !team.Allowed(readCommands, command) {
		return false
	}
	unquoted := strings.NewReplacer(`'`, "", `"`, "")
	for _, word := range strings.Fields(command) {
		option, _, _ := strings.Cut(unquoted.Replace(word), "=")
		if len(option) > 2 && strings.HasPrefix(option, "--") && slices.ContainsFunc(escapes, func(e string) bool { return strings.HasPrefix(e, option) }) {
			return false
		}
	}
	return true
}

// refuses is whether pocketd answers a reviewer's ask for d deny by itself.
func refuses(d proto.ToolDetail) bool {
	switch d.Kind {
	case "edit", "write":
		return true
	case "shell":
		return !ReadOnly(d.Command)
	}
	return false
}

func (e *Engine) setReviewer(agentID, ticket string) {
	e.reviewMu.Lock()
	defer e.reviewMu.Unlock()
	if e.reviewers == nil {
		e.reviewers = map[string]string{}
	}
	e.reviewers[agentID] = ticket
}

func (e *Engine) dropReviewer(agentID string) {
	e.reviewMu.Lock()
	defer e.reviewMu.Unlock()
	delete(e.reviewers, agentID)
}

func (e *Engine) reviewerOf(agentID string) (ticket string, ok bool) {
	e.reviewMu.Lock()
	defer e.reviewMu.Unlock()
	ticket, ok = e.reviewers[agentID]
	return ticket, ok
}

// Deny is the Broker.Deny hook: a reviewer's ask to edit, to write, or to run a command that is not
// read-only is refused and noted.
func (e *Engine) Deny(req proto.PermissionRequest) (string, bool) {
	if e == nil {
		return "", false
	}
	ticket, ok := e.reviewerOf(req.AgentID)
	if !ok || !refuses(req.Detail) {
		return "", false
	}
	text := "Refused: " + req.Detail.Command
	if req.Detail.Kind != "shell" {
		text = "Refused to " + req.Detail.Kind + " " + req.Detail.Path
	}
	e.note(ticket, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationPR, Actor: "reviewer", Kind: "denied", Text: text})
	return ReadOnlyReason, true
}

// Reads is whether req is a reviewer's read-only command, which Broker.Allow answers without an Ask.
func (e *Engine) Reads(req proto.PermissionRequest) bool {
	if e == nil {
		return false
	}
	_, ok := e.reviewerOf(req.AgentID)
	return ok && req.Detail.Kind == "shell" && ReadOnly(req.Detail.Command)
}

// ended is whether s is a session whose turn is over.
func ended(s Session) bool {
	return s.Present && (s.Turn == "idle" || s.Turn == "done")
}

// atPR is Next for a Ticket at PR review. The table in docs/plans/2026-11-12-light-factory-6-review.md lists
// its rows in this order.
func atPR(f Facts) (proto.Ticket, []Action) {
	t := f.Ticket
	if f.PR == nil || !f.Config.Reviewer {
		return t, nil
	}
	switch f.Review {
	case ReviewNone:
		return working(t), []Action{{Kind: ActStartReview}}
	case ReviewRunning, ReviewRepaired:
		if f.Review == ReviewRunning && ended(f.Reviewer) {
			return working(t), []Action{{Kind: ActRepairReview}}
		}
		return working(t), nil
	}
	if f.Reviewer.Present {
		return t, []Action{{Kind: ActCloseReviewer}}
	}
	return t, nil
}
