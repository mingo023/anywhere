package automation

import (
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestStepMapsWhatTheAgentDoesToTheRunsStatus(t *testing.T) {
	for _, c := range []struct {
		name    string
		from    string
		seen    Seen
		present bool
		worked  bool
		want    string
		why     string
	}{
		{"working", "running", Seen{Status: "working"}, true, false, "running", ""},
		{"working again after waiting", "waiting", Seen{Status: "working"}, true, true, "running", ""},
		{"needs you", "running", Seen{Status: "needsYou"}, true, true, "waiting", ""},
		{"needs you before it was seen working", "running", Seen{Status: "needsYou"}, true, false, "waiting", ""},
		{"idle after work", "running", Seen{Status: "idle", Summary: "Done."}, true, true, "succeeded", ""},
		{"done after work", "running", Seen{Status: "done", Summary: "Done."}, true, true, "succeeded", ""},
		{"done though never seen working", "running", Seen{Status: "done", Summary: "Done."}, true, false, "succeeded", ""},
		{"failed turn", "running", Seen{Status: "done", Failed: true}, true, true, "failed", "The agent failed"},
		{"idle with a reply though never seen working", "running", Seen{Status: "idle", Summary: "Done.", Answered: true}, true, false, "succeeded", ""},
		{"idle before work stays running", "running", Seen{Status: "idle"}, true, false, "running", ""},
		{"absent agent", "running", Seen{}, false, true, "cancelled", "Session closed"},
		{"absent while waiting", "waiting", Seen{}, false, true, "cancelled", "Session closed"},
		{"closed agent", "running", Seen{Status: "closed"}, true, true, "cancelled", "Session closed"},
		{"absent agent of a finished run", "succeeded", Seen{}, false, true, "succeeded", ""},
	} {
		got, _ := Step(proto.Run{Status: c.from}, c.seen, c.present, c.worked)
		if got.Status != c.want || got.Why != c.why {
			t.Errorf("%s: %q (%q), want %q (%q)", c.name, got.Status, got.Why, c.want, c.why)
		}
	}
}

func TestStepKeepsTheSummaryOnlyOnceTheTurnEnded(t *testing.T) {
	got, changed := Step(proto.Run{Status: "waiting"}, Seen{Status: "working", Summary: "early"}, true, true)
	if got.Summary != "" || !changed {
		t.Fatalf("%+v", got)
	}
	got, _ = Step(proto.Run{Status: "running"}, Seen{Status: "done", Summary: "Reviewed 3 PRs."}, true, true)
	if got.Summary != "Reviewed 3 PRs." {
		t.Fatalf("%+v", got)
	}
}

func TestStepReportsNoChangeWhenTheRunAlreadyLooksLikeTheAgent(t *testing.T) {
	if _, changed := Step(proto.Run{Status: "running"}, Seen{Status: "working"}, true, true); changed {
		t.Fatal("running to running changed")
	}
}

func TestPlainJoinsLinesAndCutsToTheLimit(t *testing.T) {
	if got := plain("  one\n\ntwo   three ", 240); got != "one two three" {
		t.Fatalf("%q", got)
	}
	got := plain(strings.Repeat("é", 300), 240)
	if r := []rune(got); len(r) != 240 || r[239] != '…' {
		t.Fatalf("%d runes, ends %q", len(r), string(r[len(r)-1]))
	}
}

func TestAnswerIsAReplyAfterThePrompt(t *testing.T) {
	prompt, reply, tool := proto.Item{Kind: "user"}, proto.Item{Kind: "assistant"}, proto.Item{Kind: "tool"}
	for name, items := range map[string][]proto.Item{
		"nothing yet":             nil,
		"only the prompt":         {prompt},
		"prompt then a tool":      {prompt, tool},
		"prompt then a reply":     {prompt, tool, reply},
		"a reply to an old turn":  {prompt, reply, prompt},
		"a reply past the window": {reply},
	} {
		want := name == "prompt then a reply" || name == "a reply past the window"
		if got := answered(items); got != want {
			t.Errorf("%s: %v, want %v", name, got, want)
		}
	}
}
