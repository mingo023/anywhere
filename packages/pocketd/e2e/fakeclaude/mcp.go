package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"slices"
	"strings"
)

// callMCP runs "mcp <tool> <json args>" against the server in --mcp-config,
// as claude would, and gives the tool's text result.
func callMCP(line string) string {
	tool, args, _ := strings.Cut(line, " ")
	i := slices.IndexFunc(os.Args, func(a string) bool { return strings.HasPrefix(a, "--mcp-config=") })
	if i < 0 {
		return "no mcp config"
	}
	raw, err := os.ReadFile(strings.TrimPrefix(os.Args[i], "--mcp-config="))
	if err != nil {
		return err.Error()
	}
	var cfg struct {
		McpServers map[string]struct {
			URL     string            `json:"url"`
			Headers map[string]string `json:"headers"`
		} `json:"mcpServers"`
	}
	json.Unmarshal(raw, &cfg)
	server := cfg.McpServers["anywhere"]
	if args == "" {
		args = "{}"
	}
	body := fmt.Sprintf(`{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":%q,"arguments":%s}}`, tool, args)
	req, _ := http.NewRequest(http.MethodPost, server.URL, bytes.NewBufferString(body))
	for k, v := range server.Headers {
		req.Header.Set(k, v)
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err.Error()
	}
	defer resp.Body.Close()
	var out struct {
		Result struct {
			Content []struct {
				Text string `json:"text"`
			} `json:"content"`
		} `json:"result"`
	}
	json.NewDecoder(resp.Body).Decode(&out)
	if len(out.Result.Content) == 0 {
		return fmt.Sprintf("mcp status %d", resp.StatusCode)
	}
	return out.Result.Content[0].Text
}
