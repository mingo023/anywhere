// Command fakeclaude stands in for the claude CLI in e2e tests. It writes
// transcript lines in Claude Code's JSONL shape and calls the hooks of the
// plugins in CLAUDE_CODE_PLUGIN_DIRS the way Claude Code does. Without
// CLAUDE_CONFIG_DIR it writes no transcript.
//
// Input lines:
//
//	<text>      reply "echo: <text>"
//	run <cmd>   ask the PermissionRequest hook, then run a Bash tool with its answer
//	desk <cmd>  ask the hook, but answer on the "desktop" before it replies
package main

import (
	"bufio"
	"bytes"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"maps"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"time"
)

type session struct {
	id, cwd, transcript string
	f                   *os.File
	hooks               map[string]string
}

func start() *session {
	cwd, _ := os.Getwd()
	s := &session{id: uuid(), cwd: cwd, hooks: pluginHooks()}
	if dir := os.Getenv("CLAUDE_CONFIG_DIR"); dir != "" {
		slug := regexp.MustCompile(`[^a-zA-Z0-9]`).ReplaceAllString(cwd, "-")
		s.transcript = filepath.Join(dir, "projects", slug, s.id+".jsonl")
		os.MkdirAll(filepath.Dir(s.transcript), 0o700)
		f, err := os.OpenFile(s.transcript, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
		if err != nil {
			panic(err)
		}
		s.f = f
	}
	return s
}

func pluginHooks() map[string]string {
	hooks := map[string]string{}
	for _, dir := range strings.Split(os.Getenv("CLAUDE_CODE_PLUGIN_DIRS"), ":") {
		var file struct {
			Hooks map[string][]struct {
				Hooks []struct{ Command string }
			}
		}
		b, err := os.ReadFile(filepath.Join(dir, "hooks", "hooks.json"))
		if err != nil || json.Unmarshal(b, &file) != nil {
			continue
		}
		for event, groups := range file.Hooks {
			if len(groups) > 0 && len(groups[0].Hooks) > 0 {
				hooks[event] = groups[0].Hooks[0].Command
			}
		}
	}
	return hooks
}

func (s *session) hook(event string, fields map[string]any) (wait func() []byte) {
	command := s.hooks[event]
	if command == "" {
		return func() []byte { return nil }
	}
	payload := map[string]any{"session_id": s.id, "transcript_path": s.transcript, "cwd": s.cwd, "hook_event_name": event}
	maps.Copy(payload, fields)
	raw, _ := json.Marshal(payload)
	cmd := exec.Command("sh", "-c", command)
	cmd.Stdin = bytes.NewReader(raw)
	var out bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, os.Stderr
	cmd.Start()
	return func() []byte {
		cmd.Wait()
		return out.Bytes()
	}
}

func uuid() string {
	var b [16]byte
	rand.Read(b[:])
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:])
}

func (s *session) write(line map[string]any) {
	if s.f == nil {
		return
	}
	line["uuid"] = uuid()
	line["sessionId"] = s.id
	line["timestamp"] = time.Now().UTC().Format(time.RFC3339Nano)
	b, _ := json.Marshal(line)
	s.f.Write(append(b, '\n'))
}

func user(content any) map[string]any {
	return map[string]any{"type": "user", "message": map[string]any{"role": "user", "content": content}}
}

func assistant(blocks ...map[string]any) map[string]any {
	return map[string]any{"type": "assistant", "message": map[string]any{
		"id": "msg_" + uuid(), "role": "assistant", "model": "fake-model", "content": blocks,
	}}
}

func toolResult(id, content string, isError bool) map[string]any {
	return user([]map[string]any{{"type": "tool_result", "tool_use_id": id, "content": content, "is_error": isError}})
}

func runTool(s *session, id, command string, desktop bool) {
	input := map[string]any{"command": command}
	s.write(assistant(map[string]any{"type": "tool_use", "id": id, "name": "Bash", "input": input}))
	wait := s.hook("PermissionRequest", map[string]any{"tool_name": "Bash", "tool_input": input})

	if desktop {
		time.Sleep(500 * time.Millisecond)
		fmt.Println("desktop: allow")
		s.write(toolResult(id, "ran: "+command, false))
		fmt.Printf("hook released: %q\n", wait())
		return
	}

	var reply struct {
		HookSpecificOutput struct {
			Decision struct{ Behavior, Message string }
		}
	}
	json.Unmarshal(wait(), &reply)
	d := reply.HookSpecificOutput.Decision
	fmt.Println("hook:", d.Behavior)
	if d.Behavior == "allow" {
		s.write(toolResult(id, "ran: "+command, false))
	} else {
		s.write(toolResult(id, d.Message, true))
	}
}

func main() {
	if slices.Contains(os.Args, "-p") {
		fmt.Println(`{"title":"Say hello","branchName":"say-hello","vague":false}`)
		return
	}
	if i := slices.Index(os.Args, "--model"); i > 0 && i+1 < len(os.Args) && os.Args[i+1] == "bogus" {
		fmt.Println("fake claude: unknown model bogus")
		os.Exit(1)
	}
	s := start()
	s.hook("SessionStart", map[string]any{"source": "startup", "model": "fake-model"})()
	fmt.Println("fake claude ready")

	sc := bufio.NewScanner(os.Stdin)
	first := prompt()
	for n := 1; first != "" || sc.Scan(); n++ {
		line := strings.TrimSpace(sc.Text())
		if first != "" {
			line, first = first, ""
		}
		if line == "" {
			continue
		}
		s.hook("UserPromptSubmit", map[string]any{"prompt": line})()
		s.write(user(line))
		id := fmt.Sprintf("toolu_%d", n)
		switch {
		case strings.HasPrefix(line, "run "):
			runTool(s, id, strings.TrimPrefix(line, "run "), false)
		case strings.HasPrefix(line, "desk "):
			runTool(s, id, strings.TrimPrefix(line, "desk "), true)
		default:
			fmt.Println("echo: " + line)
			s.write(assistant(map[string]any{"type": "text", "text": "echo: " + line}))
		}
		s.write(map[string]any{"type": "system", "subtype": "turn_duration", "durationMs": 5, "messageCount": 2})
		s.hook("Stop", nil)()
	}
}

// prompt is the argument after --, as pocketd passes a Session's first prompt.
func prompt() string {
	if i := slices.Index(os.Args, "--"); i > 0 && i+1 < len(os.Args) {
		return os.Args[i+1]
	}
	return ""
}
