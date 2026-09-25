// Command fakeclaude stands in for the claude CLI in e2e tests. It writes
// transcript lines in Claude Code's JSONL shape and calls the
// PermissionRequest hook from --settings the way Claude Code does.
//
// Input lines:
//
//	<text>      reply "echo: <text>"
//	run <cmd>   ask the hook, then run a Bash tool with its answer
//	desk <cmd>  ask the hook, but answer on the "desktop" before it replies
package main

import (
	"bufio"
	"bytes"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

type transcript struct {
	f       *os.File
	session string
}

func openTranscript(sessionID, cwd string) *transcript {
	if sessionID == "" {
		return &transcript{}
	}
	slug := regexp.MustCompile(`[^a-zA-Z0-9]`).ReplaceAllString(cwd, "-")
	dir := filepath.Join(os.Getenv("CLAUDE_CONFIG_DIR"), "projects", slug)
	os.MkdirAll(dir, 0o700)
	f, err := os.OpenFile(filepath.Join(dir, sessionID+".jsonl"), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		panic(err)
	}
	return &transcript{f: f, session: sessionID}
}

func uuid() string {
	var b [16]byte
	rand.Read(b[:])
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:])
}

func (t *transcript) write(line map[string]any) {
	if t.f == nil {
		return
	}
	line["uuid"] = uuid()
	line["sessionId"] = t.session
	line["timestamp"] = time.Now().UTC().Format(time.RFC3339Nano)
	b, _ := json.Marshal(line)
	t.f.Write(append(b, '\n'))
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

func hookCommand(settings string) string {
	var s struct {
		Hooks struct {
			PermissionRequest []struct {
				Hooks []struct{ Command string }
			}
		}
	}
	b, err := os.ReadFile(settings)
	if err != nil || json.Unmarshal(b, &s) != nil || len(s.Hooks.PermissionRequest) == 0 {
		panic("no PermissionRequest hook in " + settings)
	}
	return s.Hooks.PermissionRequest[0].Hooks[0].Command
}

func runTool(tr *transcript, settings, sessionID, cwd, id, command string, desktop bool) {
	input := map[string]any{"command": command}
	tr.write(assistant(map[string]any{"type": "tool_use", "id": id, "name": "Bash", "input": input}))
	payload, _ := json.Marshal(map[string]any{
		"session_id": sessionID, "hook_event_name": "PermissionRequest", "cwd": cwd,
		"tool_name": "Bash", "tool_input": input,
	})
	hook := exec.Command("sh", "-c", hookCommand(settings))
	hook.Stdin = bytes.NewReader(payload)
	var out bytes.Buffer
	hook.Stdout = &out
	hook.Stderr = os.Stderr

	if desktop {
		hook.Start()
		time.Sleep(500 * time.Millisecond)
		fmt.Println("desktop: allow")
		tr.write(toolResult(id, "ran: "+command, false))
		hook.Wait()
		fmt.Printf("hook released: %q\n", out.String())
		return
	}

	hook.Run()
	var reply struct {
		HookSpecificOutput struct {
			Decision struct{ Behavior, Message string }
		}
	}
	json.Unmarshal(out.Bytes(), &reply)
	d := reply.HookSpecificOutput.Decision
	fmt.Println("hook:", d.Behavior)
	if d.Behavior == "allow" {
		tr.write(toolResult(id, "ran: "+command, false))
	} else {
		tr.write(toolResult(id, d.Message, true))
	}
}

func main() {
	var sessionID, settings string
	for i := 1; i+1 < len(os.Args); i++ {
		switch os.Args[i] {
		case "--session-id":
			sessionID = os.Args[i+1]
		case "--settings":
			settings = os.Args[i+1]
		}
	}
	cwd, _ := os.Getwd()
	tr := openTranscript(sessionID, cwd)
	fmt.Println("fake claude ready")

	sc := bufio.NewScanner(os.Stdin)
	for n := 1; sc.Scan(); n++ {
		line := strings.TrimSpace(sc.Text())
		if line == "" {
			continue
		}
		tr.write(user(line))
		id := fmt.Sprintf("toolu_%d", n)
		switch {
		case strings.HasPrefix(line, "run "):
			runTool(tr, settings, sessionID, cwd, id, strings.TrimPrefix(line, "run "), false)
		case strings.HasPrefix(line, "desk "):
			runTool(tr, settings, sessionID, cwd, id, strings.TrimPrefix(line, "desk "), true)
		default:
			fmt.Println("echo: " + line)
			tr.write(assistant(map[string]any{"type": "text", "text": "echo: " + line}))
		}
		tr.write(map[string]any{"type": "system", "subtype": "turn_duration", "durationMs": 5, "messageCount": 2})
	}
}
