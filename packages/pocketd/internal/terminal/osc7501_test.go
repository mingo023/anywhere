package terminal

import (
	"encoding/base64"
	"strings"
	"testing"
)

func TestScannerReadsTheRootReportsInAChunk(t *testing.T) {
	var s scanner
	got, query := s.feed([]byte("x\x1b]7501;state=working:app=claude-code\x1b\\y\x1b]7501;state=blocked:kind=permission:msg=aGk=\x07"))
	want := []Report{{State: "working", App: "claude-code"}, {State: "blocked", Kind: "permission", Msg: "hi"}}
	if query || len(got) != 2 || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("got %+v query=%v", got, query)
	}
}

func TestScannerJoinsASequenceSplitAcrossReads(t *testing.T) {
	var s scanner
	parts := []string{"\x1b]75", "01;state=do", "ne:app=x\x1b", "\\"}
	for _, part := range parts[:len(parts)-1] {
		if got, _ := s.feed([]byte(part)); len(got) != 0 {
			t.Fatalf("%q already gave %+v", part, got)
		}
	}
	got, _ := s.feed([]byte(parts[len(parts)-1]))
	if len(got) != 1 || got[0].State != "done" {
		t.Fatalf("got %+v", got)
	}
}

func TestScannerSeesTheSupportQuery(t *testing.T) {
	var s scanner
	if got, query := s.feed([]byte("\x1b]7501;?\x1b\\\x1b[c")); !query || len(got) != 0 {
		t.Fatalf("got %+v query=%v", got, query)
	}
}

func TestScannerDropsWhatTheSpecRejects(t *testing.T) {
	var s scanner
	for _, bad := range []string{
		"state=nap",              // unknown state
		"app=x",                  // no state
		"state=working:id=sub/1", // not the root record
	} {
		if got, _ := s.feed([]byte("\x1b]7501;" + bad + "\x07")); len(got) != 0 {
			t.Errorf("%q gave %+v", bad, got)
		}
	}
}

func TestScannerSkipsABadMsgAndKeepsTheRest(t *testing.T) {
	var s scanner
	for name, msg := range map[string]string{
		"bad base64":       "!!!",
		"a control char":   base64.StdEncoding.EncodeToString([]byte("good\n")),
		"over the msg cap": base64.StdEncoding.EncodeToString([]byte(strings.Repeat("a", maxMsg+1))),
	} {
		got, _ := s.feed([]byte("\x1b]7501;state=done:msg=" + msg + ":app=claude-code\x07"))
		if len(got) != 1 || got[0] != (Report{State: "done", App: "claude-code"}) {
			t.Errorf("%s: got %+v", name, got)
		}
	}
}

func TestScannerKeepsAMsgAtTheCap(t *testing.T) {
	var s scanner
	msg := strings.Repeat("a", maxMsg)
	got, _ := s.feed([]byte("\x1b]7501;state=done:msg=" + base64.StdEncoding.EncodeToString([]byte(msg)) + "\x07"))
	if len(got) != 1 || got[0].Msg != msg {
		t.Fatalf("got %d reports", len(got))
	}
}

func sequenceOf(n int) string {
	head, tail := string(oscStart)+"state=done:app=", "\x07"
	return head + strings.Repeat("x", n-len(head)-len(tail)) + tail
}

func feedSplitAt(s *scanner, seq string, cut int) (reports []Report) {
	for _, part := range []string{seq[:cut], seq[cut:]} {
		got, _ := s.feed([]byte(part))
		reports = append(reports, got...)
	}
	return reports
}

func TestScannerAcceptsASequenceAtTheLimitHoweverItArrives(t *testing.T) {
	seq := sequenceOf(maxSequence)
	for _, cut := range []int{0, 3, 100, len(seq) - 1} {
		var s scanner
		if got := feedSplitAt(&s, seq, cut); len(got) != 1 || got[0].State != "done" {
			t.Errorf("cut at %d: got %d reports", cut, len(got))
		}
	}
}

func TestScannerDropsASequenceOverTheLimitHoweverItArrives(t *testing.T) {
	seq := sequenceOf(maxSequence + 1)
	for _, cut := range []int{0, 3, 100, len(seq) - 1} {
		var s scanner
		if got := feedSplitAt(&s, seq, cut); len(got) != 0 {
			t.Errorf("cut at %d: got %d reports", cut, len(got))
		}
		if got, _ := s.feed([]byte("\x1b]7501;state=idle\x07")); len(got) != 1 || got[0].State != "idle" {
			t.Errorf("cut at %d: the next sequence gave %+v", cut, got)
		}
	}
}

func TestScannerRecoversAfterATruncatedSequence(t *testing.T) {
	var s scanner
	got, _ := s.feed([]byte("\x1b]7501;state=wo\x1b]7501;state=idle\x07"))
	if len(got) != 1 || got[0].State != "idle" {
		t.Fatalf("got %+v", got)
	}
}
