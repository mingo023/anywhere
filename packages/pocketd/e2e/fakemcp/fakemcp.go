// Package fakemcp is the MCP client the fake agents use to reach Anywhere's
// server, as the real agents do with the config they are launched with.
package fakemcp

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"slices"
	"sort"
	"strings"
)

// Server is one MCP server an agent was launched with. Enabled and Disabled
// are Codex's per-server tool filters; both empty means every tool.
type Server struct {
	URL      string
	Headers  map[string]string
	Enabled  []string
	Disabled []string
}

// Offers is whether the agent may use tool on s.
func (s Server) Offers(tool string) bool {
	if slices.Contains(s.Disabled, tool) {
		return false
	}
	return len(s.Enabled) == 0 || slices.Contains(s.Enabled, tool)
}

func (s Server) post(body string) (*http.Response, error) {
	req, err := http.NewRequest(http.MethodPost, s.URL, bytes.NewBufferString(body))
	if err != nil {
		return nil, err
	}
	for k, v := range s.Headers {
		req.Header.Set(k, v)
	}
	req.Header.Set("Content-Type", "application/json")
	return http.DefaultClient.Do(req)
}

// Call runs tool with args (JSON; empty for none) and gives its text result.
func (s Server) Call(tool, args string) string {
	if args == "" {
		args = "{}"
	}
	resp, err := s.post(fmt.Sprintf(`{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":%q,"arguments":%s}}`, tool, args))
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

// List gives the names of the tools the server lists that the agent may use.
func (s Server) List() ([]string, error) {
	resp, err := s.post(`{"jsonrpc":"2.0","id":1,"method":"tools/list"}`)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("mcp status %d", resp.StatusCode)
	}
	var out struct {
		Result struct {
			Tools []struct {
				Name string `json:"name"`
			} `json:"tools"`
		} `json:"result"`
	}
	json.NewDecoder(resp.Body).Decode(&out)
	var names []string
	for _, t := range out.Result.Tools {
		if s.Offers(t.Name) {
			names = append(names, t.Name)
		}
	}
	return names, nil
}

// Run does one script line, "<tool> <json>" or "tools/list", against
// servers: a tool goes to the first server that offers it.
func Run(servers []Server, line string) string {
	tool, args, _ := strings.Cut(strings.TrimSpace(line), " ")
	if len(servers) == 0 {
		return "no mcp config"
	}
	if tool == "tools/list" {
		var all []string
		for _, s := range servers {
			names, err := s.List()
			if err != nil {
				return err.Error()
			}
			all = append(all, names...)
		}
		sort.Strings(all)
		return strings.Join(slices.Compact(all), " ")
	}
	for _, s := range servers {
		if s.Offers(tool) {
			return s.Call(tool, args)
		}
	}
	return "no mcp server offers " + tool
}
