package daemon

import (
	"encoding/json"
	"reflect"
	"testing"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

func TestClaudesStreamDraftsEachBlockOfItsOwnMessages(t *testing.T) {
	s := newStream()
	var got []timeline.Event
	for _, raw := range []string{
		`{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"message_start","message":{}}}`,
		`{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Hmm"}}}`,
		`{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"x"}}}`,
		`{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hi"}}}`,
		`{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{"}}}`,
		`{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"message_start","message":{}}}`,
		`{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Done"}}}`,
	} {
		var l streamLine
		if err := json.Unmarshal([]byte(raw), &l); err != nil {
			t.Fatal(err)
		}
		if e, ok := s.draft(l); ok {
			got = append(got, e)
		}
	}
	want := []timeline.Event{
		{Kind: "thinking_draft", Text: "Hmm", Draft: "1:0"},
		{Kind: "assistant_draft", Text: "Hi", Draft: "1:1"},
		{Kind: "assistant_draft", Text: "Done", Draft: "2:0"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got  %+v\nwant %+v", got, want)
	}
}

func TestASubagentsStreamIsNoDraft(t *testing.T) {
	var l streamLine
	json.Unmarshal([]byte(`{"type":"stream_event","parent_tool_use_id":"toolu_1","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}}`), &l)
	if l.ParentToolUseID == nil || *l.ParentToolUseID != "toolu_1" {
		t.Fatalf("parent: %v", l.ParentToolUseID)
	}
}

func TestClaudesMcpStatusReadsInTheWiresTerms(t *testing.T) {
	got := claudeMcp(json.RawMessage(`{"mcpServers":[
		{"name":"fff","status":"connected","scope":"user"},
		{"name":"claude.ai Linear","status":"needs-auth","scope":"claudeai"},
		{"name":"docs","status":"pending","scope":"project"},
		{"name":"new","status":"warming"}]}`))
	want := []proto.McpServer{
		{Name: "fff", Status: "connected", Scope: "user"},
		{Name: "claude.ai Linear", Status: "needsAuth", Scope: "claude.ai"},
		{Name: "docs", Status: "pending", Scope: "project"},
		{Name: "new", Status: "pending"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("%+v", got)
	}
}

func TestABrowserSignInIsForgottenOnceItsServerNeedsNone(t *testing.T) {
	s := newStream()
	s.signIns["linear"], s.signIns["asana"], s.signIns["gone"] = true, true, true
	s.signedIn([]proto.McpServer{{Name: "linear", Status: "connected"}, {Name: "asana", Status: "needsAuth"}})
	if got := s.signingIn(); !reflect.DeepEqual(got, []string{"asana"}) {
		t.Fatalf("%v", got)
	}
}
