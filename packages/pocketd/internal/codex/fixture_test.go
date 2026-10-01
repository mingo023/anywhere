package codex

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

type recorded struct {
	Thread struct {
		Turns []turn `json:"turns"`
	} `json:"thread"`
	TurnsBackwardsCursor *string `json:"turnsBackwardsCursor"`
	Data                 []turn  `json:"data"`
}

func fixture(t *testing.T, name string) recorded {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("testdata", name+".json"))
	if err != nil {
		t.Fatal(err)
	}
	var r recorded
	if err := json.Unmarshal(raw, &r); err != nil {
		t.Fatalf("%s: %v", name, err)
	}
	return r
}

func TestTheRecordedFixturesDecode(t *testing.T) {
	read, resume, page := fixture(t, "thread_read"), fixture(t, "thread_resume"), fixture(t, "turns_list")
	if len(read.Thread.Turns) == 0 || len(resume.Thread.Turns) == 0 || len(page.Data) == 0 {
		t.Fatal("a fixture holds no turns")
	}
	for _, tn := range append(read.Thread.Turns, page.Data...) {
		if tn.ID == "" {
			t.Fatalf("turn without an id: %+v", tn)
		}
	}
	if resume.TurnsBackwardsCursor == nil && len(resume.Thread.Turns) != len(read.Thread.Turns) {
		t.Fatalf("resume has %d turns, read %d, and no cursor", len(resume.Thread.Turns), len(read.Thread.Turns))
	}
}

type ignore struct{}

func (ignore) Notify(string, json.RawMessage)                   {}
func (ignore) Request(json.RawMessage, string, json.RawMessage) {}

var keep = map[string]bool{"id": true, "type": true, "status": true, "kind": true, "role": true,
	"nextCursor": true, "backwardsCursor": true, "turnsBackwardsCursor": true, "itemsBackwardsCursor": true}

// scrub blanks every string a conversation could hold, so a fixture keeps
// the shape of a thread, not its text or paths.
func scrub(v any) any {
	switch x := v.(type) {
	case map[string]any:
		for k, e := range x {
			if s, ok := e.(string); ok && s != "" && !keep[k] {
				x[k] = "x"
			} else {
				x[k] = scrub(e)
			}
		}
	case []any:
		for i, e := range x {
			if s, ok := e.(string); ok && s != "" {
				x[i] = "x"
			} else {
				x[i] = scrub(e)
			}
		}
	}
	return v
}

func TestScrubBlanksStringsInArrays(t *testing.T) {
	out, _ := json.Marshal(scrub(map[string]any{"id": "t1", "roots": []any{"/Users/me"}}))
	if string(out) != `{"id":"t1","roots":["x"]}` {
		t.Fatal(string(out))
	}
}

// TestRecordFixtures rewrites testdata from a scratch app-server; Task 4.1 of
// docs/plans/2026-09-30-restore.md says how to run one.
func TestRecordFixtures(t *testing.T) {
	sock, thread := os.Getenv("CODEX_FIXTURE_SOCK"), os.Getenv("CODEX_FIXTURE_THREAD")
	if sock == "" || thread == "" {
		t.Skip("CODEX_FIXTURE_SOCK and CODEX_FIXTURE_THREAD are unset")
	}
	ctx := context.Background()
	c, err := Dial(ctx, sock, ignore{})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	record := func(name, method string, params map[string]any) json.RawMessage {
		res, err := c.Call(ctx, method, params)
		if err != nil {
			t.Fatalf("%s: %v", method, err)
		}
		var v any
		json.Unmarshal(res, &v)
		out, _ := json.MarshalIndent(scrub(v), "", "  ")
		if err := os.WriteFile(filepath.Join("testdata", name+".json"), append(out, '\n'), 0o644); err != nil {
			t.Fatal(err)
		}
		return res
	}
	record("thread_read", "thread/read", map[string]any{"threadId": thread, "includeTurns": true})
	var r recorded
	json.Unmarshal(record("thread_resume", "thread/resume", map[string]any{"threadId": thread}), &r)
	params := map[string]any{"threadId": thread, "sortDirection": "desc", "itemsView": "full"}
	if r.TurnsBackwardsCursor != nil {
		params["cursor"] = *r.TurnsBackwardsCursor
	}
	record("turns_list", "thread/turns/list", params)
}
