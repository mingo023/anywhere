// Package catalog asks an agent's CLI for the models it offers, as the
// desktop's model pickers do (agents::providers), for agents whose sessions
// don't list them as they open.
package catalog

import (
	"bufio"
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"os/exec"
	"time"

	"pocketd/internal/providers"
)

// Wait bounds one ask, as the desktop's does.
var Wait = 45 * time.Second

// An ask runs a CLI with args and writes input. Its read takes each line the CLI
// prints and, once that is the answer, gives the models it lists and the line
// that asks for more, if any.
type ask struct {
	args  []string
	input string
	read  func(line []byte) (models []string, more string, answered bool)
}

const piInput = `{"type":"get_available_models"}`

var asks = map[string]ask{
	// Asks without a prompt, so it costs no tokens; no hooks, so the user's don't take it for a session.
	providers.Claude.ID: {
		args:  []string{"-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--no-session-persistence", "--settings", `{"disableAllHooks":true}`},
		input: `{"type":"control_request","request_id":"models","request":{"subtype":"initialize"}}`,
		read:  claude,
	},
	providers.Codex.ID: {
		args:  []string{"app-server"},
		input: `{"id":1,"method":"initialize","params":{"clientInfo":{"name":"codex_app_server_daemon","title":null,"version":"0"},"capabilities":null}}` + "\n" + `{"method":"initialized"}` + "\n" + codexList(""),
		read:  codex,
	},
	providers.Pi.ID: {args: []string{"--mode", "rpc", "--no-session"}, input: piInput, read: pi},
	// Pi's extensions can add models, so its list loads them; omp's only run for a session.
	providers.Omp.ID: {args: []string{"--mode", "rpc", "--no-session", "--no-extensions"}, input: piInput, read: pi},
}

// backup is what a kind offers when its CLI lists nothing: the desktop's fallback.
var backup = map[string][]string{providers.Claude.ID: {"haiku", "sonnet", "opus"}}

// Command lists provider's models with its CLI at exe; nil for a provider no
// CLI is asked of.
func Command(provider, exe string) []string {
	a, ok := asks[provider]
	if !ok {
		return nil
	}
	return append([]string{exe}, a.args...)
}

// Backup is what provider offers when its CLI lists nothing.
func Backup(provider string) []string { return backup[provider] }

// List runs argv, provider's Command, in the home folder and gives the models
// it answers with. The CLI is killed, with what it started, once it has.
func List(ctx context.Context, provider string, argv, env []string) ([]string, error) {
	a := asks[provider]
	ctx, cancel := context.WithTimeout(ctx, Wait)
	cmd := exec.CommandContext(ctx, argv[0], argv[1:]...)
	cmd.Dir, _ = os.UserHomeDir()
	cmd.Env = env
	killGroup(cmd)
	cmd.WaitDelay = time.Second
	in, err := cmd.StdinPipe()
	if err != nil {
		cancel()
		return nil, err
	}
	out, err := cmd.StdoutPipe()
	if err != nil {
		cancel()
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		cancel()
		return nil, err
	}
	defer func() {
		cancel()
		cmd.Wait()
	}()
	// Stdin stays open: a server may quit unanswered once it closes.
	io.WriteString(in, a.input+"\n")
	var models []string
	sc := bufio.NewScanner(out)
	sc.Buffer(nil, 64<<20)
	for sc.Scan() {
		listed, more, answered := a.read(sc.Bytes())
		if !answered {
			continue
		}
		models = append(models, listed...)
		if more == "" {
			return models, nil
		}
		io.WriteString(in, more+"\n")
	}
	return nil, cmp.Or(ctx.Err(), errors.New("exited without an answer"))
}

// claude reads Claude Code's answer to initialize, but "default", which only stands for one of the others.
func claude(line []byte) ([]string, string, bool) {
	var l struct {
		Type     string `json:"type"`
		Response struct {
			Response struct {
				Models []struct {
					Value string `json:"value"`
				} `json:"models"`
			} `json:"response"`
		} `json:"response"`
	}
	if json.Unmarshal(line, &l) != nil || l.Type != "control_response" {
		return nil, "", false
	}
	var models []string
	for _, m := range l.Response.Response.Models {
		if m.Value != "" && m.Value != "default" {
			models = append(models, m.Value)
		}
	}
	return models, "", true
}

func codexList(cursor string) string {
	params := map[string]any{}
	if cursor != "" {
		params["cursor"] = cursor
	}
	b, _ := json.Marshal(map[string]any{"id": 2, "method": "model/list", "params": params})
	return string(b)
}

// codex reads a page of the app-server's model/list, which leaves out hidden models.
func codex(line []byte) ([]string, string, bool) {
	var l struct {
		ID     json.RawMessage `json:"id"`
		Result struct {
			Data []struct {
				ID    string `json:"id"`
				Model string `json:"model"`
			} `json:"data"`
			NextCursor string `json:"nextCursor"`
		} `json:"result"`
	}
	if json.Unmarshal(line, &l) != nil || string(l.ID) != "2" {
		return nil, "", false
	}
	var models []string
	for _, m := range l.Result.Data {
		if id := cmp.Or(m.Model, m.ID); id != "" {
			models = append(models, id)
		}
	}
	more := ""
	if l.Result.NextCursor != "" {
		more = codexList(l.Result.NextCursor)
	}
	return models, more, true
}

// pi reads Pi's and omp's answer to get_available_models, as the "provider/id" --model takes.
func pi(line []byte) ([]string, string, bool) {
	var l struct {
		Command string `json:"command"`
		Data    struct {
			Models []struct {
				ID       string `json:"id"`
				Provider string `json:"provider"`
			} `json:"models"`
		} `json:"data"`
	}
	if json.Unmarshal(line, &l) != nil || l.Command != "get_available_models" {
		return nil, "", false
	}
	var models []string
	for _, m := range l.Data.Models {
		if m.ID != "" && m.Provider != "" {
			models = append(models, m.Provider+"/"+m.ID)
		}
	}
	return models, "", true
}
