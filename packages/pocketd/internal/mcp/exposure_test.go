package mcp

import (
	"net/http"
	"slices"
	"strings"
	"testing"
)

func TestToolSearchForQueueListsAnywhereQueueAndItStaysListed(t *testing.T) {
	f := newFixture(t)
	_, out := f.rpc(t, "/mcp", f.token, `{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"tool_search","arguments":{"query":"queue"}}}`)
	found := out["result"].(map[string]any)["structuredContent"].(map[string]any)["tools"].([]any)
	var names []string
	for _, x := range found {
		names = append(names, x.(map[string]any)["name"].(string))
	}
	if !slices.Contains(names, "anywhere_queue") {
		t.Fatalf("tool_search \"queue\" found %v", names)
	}
	for range 2 {
		_, out = f.rpc(t, "/mcp", f.token, `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`)
		if listed := toolNames(t, out); !slices.Contains(listed, "anywhere_queue") {
			t.Fatalf("tools/list = %v, want anywhere_queue to stay listed", listed)
		}
	}
}

func TestAnUnsupportedProtocolVersionHeaderIsRefusedAndASupportedOneAccepted(t *testing.T) {
	f := newFixture(t)
	post := func(version string) int {
		req, _ := http.NewRequest(http.MethodPost, f.srv.URL+"/mcp", strings.NewReader(`{"jsonrpc":"2.0","id":1,"method":"ping"}`))
		req.Header.Set("Authorization", "Bearer "+f.token)
		if version != "" {
			req.Header.Set("MCP-Protocol-Version", version)
		}
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		resp.Body.Close()
		return resp.StatusCode
	}
	for _, v := range Versions {
		if got := post(v); got != http.StatusOK {
			t.Errorf("version %s: status %d", v, got)
		}
	}
	if got := post(""); got != http.StatusOK {
		t.Errorf("no header: status %d, want 200 (the spec assumes 2025-03-26)", got)
	}
	if got := post("1999-01-01"); got != http.StatusBadRequest {
		t.Errorf("unsupported version: status %d, want 400", got)
	}
}
