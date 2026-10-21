package daemon

import (
	"encoding/json"
	"reflect"
	"testing"

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
