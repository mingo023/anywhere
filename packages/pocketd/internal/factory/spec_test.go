package factory

import (
	"errors"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestTheSpecRequestHoldsTheTemplateThenTheTicket(t *testing.T) {
	got := SpecRequest(proto.Ticket{Title: "Queue sends", Prompt: "Keep sends in a queue"})
	if !strings.HasPrefix(got, "<anywhere_factory_spec_request>\nWrite a spec for this ticket.") || !strings.HasSuffix(got, "</anywhere_factory_spec_request>\n\nTicket: Queue sends\n\nKeep sends in a queue") {
		t.Fatalf("got %q", got)
	}
}

func TestTheLastSpecBlockIsTheSpec(t *testing.T) {
	text := "You asked for:\n<anywhere_factory_spec>\n## Acceptance criteria\n- [ ] One checkable statement per line\n</anywhere_factory_spec>\nMine:\n" + goodSpec
	got, err := ParseSpec(text)
	if err != nil || !strings.HasPrefix(got, "## Brief\nSends wait in a queue.") || !strings.HasSuffix(got, "- None") {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestAReplyWithoutABlockIsRefused(t *testing.T) {
	for _, text := range []string{"", "No block here", "<anywhere_factory_spec>\nnever closed"} {
		if _, err := ParseSpec(text); err == nil || err.Error() != "there is no <anywhere_factory_spec> block" {
			t.Fatalf("%q: %v", text, err)
		}
	}
}

func TestASpecNeedsTheCriteriaHeadingAndAListItemUnderIt(t *testing.T) {
	block := func(body string) string { return "<anywhere_factory_spec>\n" + body + "\n</anywhere_factory_spec>" }
	for name, body := range map[string]string{
		"no heading":         "## Brief\nShort.",
		"heading, no item":   "## Acceptance criteria\n\n## Risks\n- None",
		"item under another": "## Acceptance criteria\nProse only.\n## Risks\n- None",
		"blank item":         "## Acceptance criteria\n- ",
	} {
		if _, err := ParseSpec(block(body)); err == nil {
			t.Errorf("%s: accepted", name)
		}
	}
	if _, err := ParseSpec(block("## acceptance criteria\n* It works")); err != nil {
		t.Errorf("a star item under a lower-case heading: %v", err)
	}
}

func TestTheRepairPromptSaysWhatIsWrong(t *testing.T) {
	want := "Your spec block can't be used: there is no <anywhere_factory_spec> block. Send the corrected <anywhere_factory_spec> block only."
	if got := RepairSpec(errors.New("there is no <anywhere_factory_spec> block")); got != want {
		t.Fatalf("got %q", got)
	}
}

func TestThePlanRequestCarriesTheSpecOnlyWhenThereIsOne(t *testing.T) {
	x := proto.Ticket{Title: "Queue sends", Prompt: "Keep sends in a queue"}
	if got := PlanRequest(x, "## Brief\nShort."); got != "Plan this ticket.\n\nTicket: Queue sends\n\nKeep sends in a queue\n\nSpec:\n## Brief\nShort." {
		t.Fatalf("with a spec: %q", got)
	}
	if got := PlanRequest(x, ""); got != "Plan this ticket.\n\nTicket: Queue sends\n\nKeep sends in a queue" {
		t.Fatalf("without: %q", got)
	}
}

func TestAfterARestartTheLastTurnSaysWhatWasAsked(t *testing.T) {
	user := func(text string) proto.Item { return proto.Item{Kind: "user", Text: text} }
	said := proto.Item{Kind: "assistant", Text: goodSpec}
	end := proto.Item{Kind: "result", OK: true}
	ask := user(Rules + "\n\n" + SpecRequest(proto.Ticket{Title: "T", Prompt: "P"}))
	repair := user(RepairSpec(errors.New("x")))
	for name, c := range map[string]struct {
		items   []proto.Item
		working bool
		want    SpecState
	}{
		"no turn":                  {nil, false, SpecNone},
		"the spec turn ended":      {[]proto.Item{ask, said, end}, false, SpecAsked},
		"the repair turn ended":    {[]proto.Item{ask, end, repair, said, end}, false, SpecRepaired},
		"the spec turn still goes": {[]proto.Item{ask, said}, true, SpecAsked},
		"the restart cut the turn": {[]proto.Item{ask, said}, false, SpecNone},
		"you typed last":           {[]proto.Item{ask, said, end, user("hello"), end}, false, SpecNone},
	} {
		if got := Recalled(c.items, c.working); got != c.want {
			t.Errorf("%s: got %s, want %s", name, got, c.want)
		}
	}
}
