// Package claude turns Claude Code's JSONL transcript into timeline events.
package claude

import (
	"bufio"
	"context"
	"encoding/json"
	"html"
	"io"
	"os"
	"strings"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

type line struct {
	Type             string `json:"type"`
	UUID             string `json:"uuid"`
	ParentUUID       string `json:"parentUuid"`
	Subtype          string `json:"subtype"`
	IsMeta           bool   `json:"isMeta"`
	IsSidechain      bool   `json:"isSidechain"`
	IsCompactSummary bool   `json:"isCompactSummary"`
	IsAPIError       bool   `json:"isApiErrorMessage"`
	Entrypoint       string `json:"entrypoint"`
	Timestamp        string `json:"timestamp"`
	DurationMs       int64  `json:"durationMs"`
	CompactMetadata  struct {
		Trigger string `json:"trigger"`
	} `json:"compactMetadata"`
	Message struct {
		Model   string          `json:"model"`
		Content json.RawMessage `json:"content"`
		Usage   *struct {
			InputTokens              int64 `json:"input_tokens"`
			CacheCreationInputTokens int64 `json:"cache_creation_input_tokens"`
			CacheReadInputTokens     int64 `json:"cache_read_input_tokens"`
		} `json:"usage"`
	} `json:"message"`
	Origin struct {
		Kind string `json:"kind"`
	} `json:"origin"`
	// ToolUseResult is a string for some tools, so it is read only when needed.
	ToolUseResult json.RawMessage `json:"toolUseResult"`
	Attachment    struct {
		Type        string          `json:"type"`
		Met         bool            `json:"met"`
		Failed      bool            `json:"failed"`
		Condition   string          `json:"condition"`
		CommandMode string          `json:"commandMode"`
		Prompt      json.RawMessage `json:"prompt"`
	} `json:"attachment"`
	CommandRun struct {
		Command string `json:"command"`
		Args    string `json:"args"`
	} `json:"commandRun"`
}

// Model returns the model an assistant line was written by, or "".
func Model(raw []byte) string {
	var l line
	if json.Unmarshal(raw, &l) != nil || l.Type != "assistant" || l.IsSidechain || l.Message.Model == "<synthetic>" {
		return ""
	}
	return l.Message.Model
}

// Goal reads the /goal a line sets, judges or clears; ok is false when it
// touches none. Claude writes a goal_status attachment for each.
func Goal(raw []byte) (g proto.Goal, ok bool) {
	var l line
	if json.Unmarshal(raw, &l) != nil || l.IsSidechain {
		return g, false
	}
	switch {
	case l.Type == "attachment" && l.Attachment.Type == "goal_status":
		g = proto.Goal{Objective: l.Attachment.Condition, Status: "active"}
		if l.Attachment.Met {
			g.Status = proto.GoalComplete
		} else if l.Attachment.Failed {
			g.Status = proto.GoalFailed
		}
		return g, true
	case l.Type == "system" && l.Subtype == "local_command" && l.CommandRun.Command == "goal" && strings.TrimSpace(l.CommandRun.Args) == "clear":
		return g, true
	}
	return g, false
}

type block struct {
	Type      string          `json:"type"`
	Text      string          `json:"text"`
	Thinking  string          `json:"thinking"`
	ID        string          `json:"id"`
	Name      string          `json:"name"`
	Input     json.RawMessage `json:"input"`
	ToolUseID string          `json:"tool_use_id"`
	Content   json.RawMessage `json:"content"`
	IsError   bool            `json:"is_error"`
}

// Map returns the events one transcript line adds.
func Map(raw []byte) []timeline.Event {
	return new(Turns).Map(raw)
}

// Turns maps a transcript's lines in order. claude -p writes no turn_duration,
// so it times those turns from their first event to their end.
type Turns struct{ start time.Time }

func (t *Turns) Map(raw []byte) []timeline.Event {
	var l line
	if json.Unmarshal(raw, &l) != nil || l.IsSidechain {
		return nil
	}
	events := l.events()
	at, _ := time.Parse(time.RFC3339Nano, l.Timestamp)
	for i, e := range events {
		if t.start.IsZero() {
			t.start = at
		}
		if e.Kind == "result" {
			if e.DurationMs == 0 && !t.start.IsZero() && !at.IsZero() {
				events[i].DurationMs = at.Sub(t.start).Milliseconds()
			}
			t.start = time.Time{}
		}
	}
	return events
}

func (l line) events() (events []timeline.Event) {
	if l.IsMeta || l.IsCompactSummary {
		return nil
	}
	switch l.Type {
	case "system":
		switch l.Subtype {
		case "turn_duration":
			return []timeline.Event{{Kind: "result", OK: true, DurationMs: l.DurationMs}}
		case "stop_hook_summary":
			// claude -p ends a turn here, as Pocket's plugin hooks Stop; the TUI writes turn_duration too.
			if l.Entrypoint == "sdk-cli" {
				return []timeline.Event{{Kind: "result", OK: true}}
			}
		case "compact_boundary":
			return []timeline.Event{{Kind: "compacted", Trigger: l.CompactMetadata.Trigger}}
		}
	case "attachment":
		// A task notification that comes mid-turn joins the turn as a queued command, not a user line.
		if l.Attachment.Type == "queued_command" && l.Attachment.CommandMode == "task-notification" {
			return taskEnd(l.Attachment.Prompt)
		}
	case "assistant":
		// claude -p ends a failed turn with the error, and no Stop.
		if l.IsAPIError && l.Entrypoint == "sdk-cli" {
			var text string
			for _, b := range blocks(l.Message.Content) {
				text += b.Text
			}
			return []timeline.Event{{Kind: "result", OK: false, Error: text}}
		}
		for _, b := range blocks(l.Message.Content) {
			switch {
			case b.Type == "text" && b.Text != "":
				events = append(events, timeline.Event{Kind: "assistant_text", Text: b.Text})
			case b.Type == "thinking" && b.Thinking != "":
				events = append(events, timeline.Event{Kind: "thinking", Text: b.Thinking})
			case b.Type == "tool_use":
				events = append(events, timeline.Event{Kind: "tool_start", ToolUseID: b.ID, Name: b.Name, Input: b.Input})
			}
		}
	case "user":
		// Claude writes harness input, like task notifications, as user lines; only a human's is a prompt.
		switch l.Origin.Kind {
		case "", "human":
		case "task-notification":
			return taskEnd(l.Message.Content)
		default:
			return nil
		}
		// A background call returns as it launches; it ends with its task notification.
		if task, ok := l.background(); ok {
			for _, b := range blocks(l.Message.Content) {
				if b.Type == "tool_result" {
					events = append(events, timeline.Event{Kind: "tool_background", ToolUseID: b.ToolUseID, TaskID: task})
				}
			}
			return events
		}
		if events, ok := l.bash(); ok {
			return events
		}
		events = userEvents(l.Message.Content)
		// Claude writes no notification for a command or workflow the agent stopped, so the stop's result ends it.
		if task, ok := l.stopped(); ok {
			events = append(events, timeline.Event{Kind: "tool_end", TaskID: task, OK: true})
		}
		return events
	}
	return events
}

// bash maps a bash mode line. Claude answers the command as it would a prompt,
// so the command starts a turn, with a call that shows its output.
func (l line) bash() ([]timeline.Event, bool) {
	var text string
	if json.Unmarshal(l.Message.Content, &text) != nil || !strings.HasPrefix(text, "<bash-") {
		return nil, false
	}
	if strings.HasPrefix(text, "<bash-input>") {
		cmd := tag(text, "bash-input")
		input, _ := json.Marshal(map[string]string{"command": cmd})
		return []timeline.Event{{Kind: "user", Text: "! " + cmd}, {Kind: "tool_start", ToolUseID: l.UUID, Name: "Bash", Input: input}}, true
	}
	// Claude keeps no exit status, and many commands that work write to stderr.
	out := strings.TrimSpace(tag(text, "bash-stdout") + "\n" + tag(text, "bash-stderr"))
	return []timeline.Event{{Kind: "tool_end", ToolUseID: l.ParentUUID, OK: true, Output: &out}}, true
}

// background says whether the call went on in the background, an agent launched async or a command, and its task id.
func (l line) background() (string, bool) {
	var r struct {
		IsAsync          bool   `json:"isAsync"`
		AgentID          string `json:"agentId"`
		BackgroundTaskID string `json:"backgroundTaskId"`
	}
	if json.Unmarshal(l.ToolUseResult, &r) != nil {
		return "", false
	}
	if r.IsAsync {
		return r.AgentID, true
	}
	return r.BackgroundTaskID, r.BackgroundTaskID != ""
}

// stopped says whether the line is a TaskStop's result, and the task it stopped.
func (l line) stopped() (string, bool) {
	var r struct {
		TaskID   string `json:"task_id"`
		TaskType string `json:"task_type"`
	}
	if json.Unmarshal(l.ToolUseResult, &r) != nil || r.TaskType == "" {
		return "", false
	}
	return r.TaskID, true
}

// taskEnd ends the tool call a background task ran under, from the notification Claude writes when it stops.
func taskEnd(content json.RawMessage) []timeline.Event {
	var text string
	json.Unmarshal(content, &text)
	id := tag(text, "tool-use-id")
	if id == "" {
		return nil
	}
	// A task the agent stopped ended as asked, not in error.
	e := timeline.Event{Kind: "tool_end", ToolUseID: id, OK: tag(text, "status") != "failed"}
	// The result is the agent's own text, which may hold the closing tag itself.
	if _, rest, ok := strings.Cut(text, "<result>"); ok {
		if end := strings.LastIndex(rest, "</result>"); end >= 0 {
			out := html.UnescapeString(strings.TrimSpace(rest[:end]))
			e.Output = &out
		}
	}
	return []timeline.Event{e}
}

func tag(text, name string) string {
	_, rest, ok := strings.Cut(text, "<"+name+">")
	value, _, closed := strings.Cut(rest, "</"+name+">")
	if !ok || !closed {
		return ""
	}
	return strings.TrimSpace(value)
}

func userEvents(content json.RawMessage) []timeline.Event {
	var text string
	var events []timeline.Event
	if json.Unmarshal(content, &text) != nil {
		for _, b := range blocks(content) {
			switch b.Type {
			case "text":
				text += b.Text
			case "tool_result":
				out := textOf(b.Content)
				events = append(events, timeline.Event{Kind: "tool_end", ToolUseID: b.ToolUseID, OK: !b.IsError, Output: &out})
			}
		}
	}
	name := tag(text, "command-name")
	switch {
	case strings.HasPrefix(text, "[Request interrupted by user"):
		return append(events, timeline.Event{Kind: "result", OK: false, Error: "interrupted"})
	// A /clear opens the conversation it starts, which has nothing of the one before to answer.
	case strings.HasPrefix(text, "<command-") && name != "" && name != "/clear":
		return append(events, timeline.Event{Kind: "user", Text: strings.TrimSpace(name + " " + tag(text, "command-args"))})
	case strings.HasPrefix(text, "<command-"), strings.HasPrefix(text, "<local-command-"), Command(text), text == "":
		return events
	}
	return append(events, timeline.Event{Kind: "user", Text: text})
}

// Command says whether text is a slash command, which newer Claude writes as typed; a prompt may start with a path instead.
func Command(text string) bool {
	rest, ok := strings.CutPrefix(strings.TrimSpace(text), "/")
	words := strings.Fields(rest)
	return ok && len(words) > 0 && !strings.ContainsAny(words[0], "/.")
}

func blocks(raw json.RawMessage) []block {
	var bs []block
	json.Unmarshal(raw, &bs)
	return bs
}

func textOf(raw json.RawMessage) string {
	var s string
	if json.Unmarshal(raw, &s) == nil {
		return s
	}
	var out strings.Builder
	for _, b := range blocks(raw) {
		out.WriteString(b.Text)
	}
	return out.String()
}

// Opening is how a transcript starts, and the prompt it is up to.
type Opening struct {
	Cleared     bool      // a /clear started it
	Interactive bool      // it was typed into Claude's input, not given to claude -p or the SDK
	Prompt      string    // its latest prompt, "" until Claude writes one
	At          time.Time // when Claude wrote Prompt, zero if it didn't say
	read        int64     // how far it was read: to the end of its last whole line
}

// longestLine is the most of a line Read looks at. A prompt holding pasted
// images runs to a few hundred KiB.
const longestLine = 4 << 20

// Read reads on in the transcript at path from where o got to.
func (o Opening) Read(path string) Opening {
	f, err := os.Open(path)
	if err != nil {
		return o
	}
	defer f.Close()
	if _, err := f.Seek(o.read, io.SeekStart); err != nil {
		return o
	}
	r := bufio.NewReaderSize(f, 64<<10)
	for {
		raw, n, err := wholeLine(r)
		if err != nil {
			return o
		}
		o.read += n
		var l line
		if json.Unmarshal(raw, &l) != nil || l.Type != "user" || l.IsSidechain {
			continue
		}
		if o.Prompt == "" {
			var text string
			json.Unmarshal(l.Message.Content, &text)
			o.Cleared = o.Cleared || tag(text, "command-name") == "/clear" || strings.TrimSpace(text) == "/clear"
			o.Interactive = l.Entrypoint == "cli"
		}
		for _, e := range l.events() {
			// The prompts it is matched against hold no commands.
			if e.Kind == "user" && !Command(e.Text) {
				o.Prompt = e.Text
				o.At, _ = time.Parse(time.RFC3339Nano, l.Timestamp)
			}
		}
	}
}

// wholeLine reads a line to its newline, keeping no more than longestLine of
// it, and how long it was. A line Claude is still writing is an io.EOF.
func wholeLine(r *bufio.Reader) (raw []byte, n int64, err error) {
	for {
		chunk, err := r.ReadSlice('\n')
		n += int64(len(chunk))
		if len(raw)+len(chunk) <= longestLine {
			raw = append(raw, chunk...)
		}
		if err != bufio.ErrBufferFull {
			return raw, n, err
		}
	}
}

const poll = 100 * time.Millisecond

// Tail waits for the file at path, then calls fn with each complete line
// until ctx ends, and with the lines already written when it does.
func Tail(ctx context.Context, path string, fn func([]byte)) {
	var f *os.File
	for f == nil {
		f, _ = os.Open(path)
		if f == nil && !sleep(ctx) {
			return
		}
	}
	defer f.Close()
	r := bufio.NewReader(f)
	var partial []byte
	for {
		chunk, err := r.ReadBytes('\n')
		partial = append(partial, chunk...)
		if err == nil {
			fn(partial[:len(partial)-1])
			partial = nil
			continue
		}
		if err != io.EOF || ctx.Err() != nil {
			return
		}
		sleep(ctx)
	}
}

func sleep(ctx context.Context) bool {
	select {
	case <-ctx.Done():
		return false
	case <-time.After(poll):
		return true
	}
}
