package share

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type reply struct {
	ID     int `json:"id"`
	Result struct {
		ProtocolVersion string `json:"protocolVersion"`
		Tools           []struct {
			Name string `json:"name"`
		} `json:"tools"`
		Content []struct {
			Type string `json:"type"`
			Text string `json:"text"`
		} `json:"content"`
		IsError bool `json:"isError"`
	} `json:"result"`
	Error *struct {
		Code int `json:"code"`
	} `json:"error"`
}

func serve(t *testing.T, dir string, lines ...string) []reply {
	t.Helper()
	var out bytes.Buffer
	if err := Serve(strings.NewReader(strings.Join(lines, "\n")), &out, dir); err != nil {
		t.Fatal(err)
	}
	var replies []reply
	for dec := json.NewDecoder(&out); dec.More(); {
		var r reply
		if err := dec.Decode(&r); err != nil {
			t.Fatal(err)
		}
		replies = append(replies, r)
	}
	return replies
}

func TestServeAnswersTheHandshakeAndListsShareFile(t *testing.T) {
	got := serve(t, t.TempDir(),
		`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-code","version":"2"}}}`,
		`{"jsonrpc":"2.0","method":"notifications/initialized"}`,
		`{"jsonrpc":"2.0","id":2,"method":"tools/list"}`,
		`{"jsonrpc":"2.0","id":3,"method":"resources/list"}`)
	if len(got) != 3 {
		t.Fatalf("%d replies; the notification gets none: %+v", len(got), got)
	}
	if got[0].ID != 1 || got[0].Result.ProtocolVersion != "2025-06-18" {
		t.Errorf("initialize = %+v", got[0])
	}
	if len(got[1].Result.Tools) != 1 || got[1].Result.Tools[0].Name != "share_file" {
		t.Errorf("tools = %+v", got[1].Result.Tools)
	}
	if got[2].Error == nil || got[2].Error.Code != -32601 {
		t.Errorf("an unknown method = %+v", got[2])
	}
}

func TestACallKeepsTheFileAndAnswersWhereOrWhyNot(t *testing.T) {
	src := filepath.Join(t.TempDir(), "report.pdf")
	os.WriteFile(src, []byte("%PDF"), 0o644)
	dir := t.TempDir()
	got := serve(t, dir,
		`{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"share_file","arguments":{"path":"`+src+`","caption":"Q3"}}}`,
		`{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"share_file","arguments":{"path":"/nope/x.png"}}}`)
	ok, failed := got[0].Result, got[1].Result
	if ok.IsError || len(ok.Content) != 1 || ok.Content[0].Type != "text" || filepath.Dir(ok.Content[0].Text) != dir || !strings.HasSuffix(ok.Content[0].Text, "-report.pdf") {
		t.Errorf("share = %+v", ok)
	}
	if !failed.IsError || !strings.Contains(failed.Content[0].Text, "There's no file at /nope/x.png") {
		t.Errorf("failed share = %+v", failed)
	}
}

func TestToolIsWhatClaudeNamesThePluginsShareFile(t *testing.T) {
	if Tool != "mcp__plugin_anywhere_anywhere__share_file" {
		t.Fatal(Tool)
	}
}
