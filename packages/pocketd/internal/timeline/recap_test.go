package timeline

import (
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func said(kind, text string) proto.Item { return proto.Item{Kind: kind, Text: text} }

func tool(kind, path string) proto.Item {
	return proto.Item{Kind: "tool", Call: &proto.ToolCall{Detail: proto.ToolDetail{Kind: kind, Path: path}}}
}

func TestARecapOfAWokenSessionKeepsThePromptTheUserLastSent(t *testing.T) {
	wake := said("user", "Delegated task t1 reached a terminal state.")
	wake.Origin = "wake"
	r := Recap([]proto.Item{said("user", "ship it"), said("assistant", "Delegated."), wake, said("assistant", "The tests pass.")})
	if r.Asked != "ship it" || r.Said != "The tests pass." {
		t.Fatalf("%+v", r)
	}
}

func TestARecapTellsOnlyTheLastTurn(t *testing.T) {
	items := []proto.Item{
		said("user", "old"),
		tool("read", "a.rs"),
		said("assistant", "old answer"),
		said("user", "fix it"),
		tool("read", "b.rs"),
		tool("read", "b.rs"),
		tool("read", "c.rs"),
		tool("edit", "b.rs"),
		tool("write", "d.rs"),
		said("assistant", "thinking"),
		said("assistant", "Fixed."),
	}
	if got, want := Recap(items), (proto.Recap{Asked: "fix it", Read: 2, Edited: 2, Said: "Fixed."}); got != want {
		t.Fatalf("%+v, want %+v", got, want)
	}
}

func TestASubAgentsWorkIsNotTheSessions(t *testing.T) {
	sub := func(it proto.Item) proto.Item {
		it.ParentToolUseID = "task"
		return it
	}
	items := []proto.Item{said("user", "go"), sub(tool("read", "x.rs")), sub(said("assistant", "sub says")), said("assistant", "done")}
	if got, want := Recap(items), (proto.Recap{Asked: "go", Said: "done"}); got != want {
		t.Fatalf("%+v, want %+v", got, want)
	}
}

func TestNoTimelineTellsNothing(t *testing.T) {
	if got := Recap(nil); got != (proto.Recap{}) {
		t.Fatalf("%+v", got)
	}
}

func TestARecapCutsAskedAndSaidAtTwoThousandCharactersWithoutSplittingOne(t *testing.T) {
	long := strings.Repeat("é", recapText+5)
	r := Recap([]proto.Item{said("user", long), said("assistant", long)})
	want := strings.Repeat("é", recapText)
	if r.Asked != want || r.Said != want {
		t.Fatalf("asked %d bytes, said %d bytes", len(r.Asked), len(r.Said))
	}
}

func TestATimelineTellsItsOwnRecap(t *testing.T) {
	tl := New()
	tl.Load([]proto.Item{{ID: "1", Seq: 1, Kind: "user", Text: "hi"}, {ID: "2", Seq: 2, Kind: "assistant", Text: "hello"}})
	if got, want := tl.Recap(), (proto.Recap{Asked: "hi", Said: "hello"}); got != want {
		t.Fatalf("%+v, want %+v", got, want)
	}
}
