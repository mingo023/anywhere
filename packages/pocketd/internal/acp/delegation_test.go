package acp

import (
	"fmt"
	"strings"
	"testing"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// transcriptEvents is what a fresh read of lines, one sub-agent's transcript, tells under call.
func transcriptEvents(call string, lines ...string) []timeline.Event {
	d := &delegation{call: call, transcripts: []string{""}, read: []int64{0}, open: map[string][]string{}, summaries: map[int]string{}}
	var events []timeline.Event
	for _, l := range lines {
		if e, ok := d.line(0, l); ok {
			events = append(events, e)
		}
	}
	return events
}

// shownAfter is what a timeline shows once the call started and the first n events reached it.
func shownAfter(call string, events []timeline.Event, n int) []proto.Item {
	tl := timeline.New()
	tl.Apply(timeline.Event{Kind: "tool_start", ToolUseID: call, Name: "delegate_task"}, 1)
	tl.Apply(timeline.Event{Kind: "user", Text: "the brief", ParentToolUseID: call}, 1)
	for _, e := range events[:n] {
		tl.Apply(e, 1)
	}
	items, _ := tl.PageWithSubagents(0, 100)
	return items
}

func TestARestoredHermesDelegationDropsOnlyTheTranscriptLinesTheTimelineShows(t *testing.T) {
	for name, lines := range map[string][]string{
		"a thought":                    {"12:00:02 think    | Listing them.", "12:00:03 assistant| Found a.txt"},
		"two assistant lines in a row": {"12:00:03 assistant| Found a.txt", "12:00:04 assistant| and b.txt"},
		"a thought between replies":    {"12:00:03 assistant| One.", "12:00:04 think    | Hmm.", "12:00:05 assistant| Two."},
	} {
		events := transcriptEvents("tc-d", lines...)
		for n := range len(events) + 1 {
			d := &delegation{call: "tc-d", shown: shownAfter("tc-d", events, n)}
			var got []string
			for _, e := range d.unseen(events) {
				got = append(got, e.Kind+":"+e.Text)
			}
			var want []string
			for _, e := range events[n:] {
				want = append(want, e.Kind+":"+e.Text)
			}
			if fmt.Sprint(got) != fmt.Sprint(want) {
				t.Errorf("%s, %d shown: unseen %s, want %s", name, n, strings.Join(got, "|"), strings.Join(want, "|"))
			}
		}
	}
}
