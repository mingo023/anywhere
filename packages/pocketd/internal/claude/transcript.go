// Package claude turns Claude Code's JSONL transcript into timeline events.
package claude

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"html"
	"io"
	"maps"
	"os"
	"regexp"
	"slices"
	"strings"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

type line struct {
	Type             string          `json:"type"`
	UUID             string          `json:"uuid"`
	ParentUUID       string          `json:"parentUuid"`
	Subtype          string          `json:"subtype"`
	IsMeta           bool            `json:"isMeta"`
	IsSidechain      bool            `json:"isSidechain"`
	IsCompactSummary bool            `json:"isCompactSummary"`
	IsAPIError       bool            `json:"isApiErrorMessage"`
	Error            string          `json:"error"`
	TurnOrigin       string          `json:"turnOrigin"`
	Entrypoint       string          `json:"entrypoint"`
	Timestamp        string          `json:"timestamp"`
	Content          json.RawMessage `json:"content"`
	DurationMs       int64           `json:"durationMs"`
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
	Origin origin `json:"origin"`
	// ToolUseResult is a string for some tools, so it is read only when needed.
	ToolUseResult json.RawMessage `json:"toolUseResult"`
	Attachment    struct {
		Type        string          `json:"type"`
		Met         bool            `json:"met"`
		Failed      bool            `json:"failed"`
		Condition   string          `json:"condition"`
		CommandMode string          `json:"commandMode"`
		Prompt      json.RawMessage `json:"prompt"`
		Origin      origin          `json:"origin"`
	} `json:"attachment"`
	CommandRun struct {
		Command string `json:"command"`
		Args    string `json:"args"`
	} `json:"commandRun"`
}

// origin says who wrote a line Claude didn't take from the human.
type origin struct {
	Kind               string `json:"kind"`
	SenderTaskID       string `json:"senderTaskId"`
	Handback           bool   `json:"handback"`
	Body               string `json:"body"`
	HandbackReportText string `json:"handbackReportText"`
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
type Turns struct {
	start time.Time
	// Claude writes a caveat before a command it runs itself, like /goal clear;
	// no model turn follows, so the command's output ends its turn.
	local, running bool
	// agents maps the task id of each agent running in the background to its call; launched keeps every agent's, for
	// one Claude resumes.
	agents, launched map[string]string
	// hooked is set while a hook that kept Claude going has ended the run it stopped,
	// and retime until the next end, which Claude times from the start of both runs.
	hooked, retime bool
	// erred is when the turn an API error ended began, until the next line shows whether Claude went on.
	erred *time.Time
}

func (t *Turns) Map(raw []byte) []timeline.Event {
	var l line
	if json.Unmarshal(raw, &l) != nil || l.IsSidechain {
		return nil
	}
	events := l.events()
	if l.Type == "system" && l.Subtype == "agents_killed" {
		events = t.killed()
	}
	t.follow(l, events)
	if t.running {
		end := timeline.Event{Kind: "result", OK: true}
		if out, ok := l.localOutput(); ok {
			t.running = false
			events = []timeline.Event{end}
			if out != "" {
				events = []timeline.Event{{Kind: "assistant_text", Text: out}, end}
			}
		} else if len(events) > 0 {
			t.running = false
			events = append([]timeline.Event{end}, events...)
		}
	}
	if t.local && len(events) > 0 && events[len(events)-1].Kind == "user" {
		t.running = true
	}
	t.local = l.caveat()
	if t.erred != nil && len(events) > 0 {
		switch events[0].Kind {
		case "assistant_text", "thinking", "tool_start":
			events = append([]timeline.Event{{Kind: "retried"}}, events...)
			t.start = *t.erred
		}
		t.erred = nil
	}
	hook := l.IsMeta && len(events) == 1 && events[0].Kind == "notice" && !t.start.IsZero()
	if hook {
		events = append([]timeline.Event{{Kind: "result", OK: true}}, events...)
	}
	at, _ := time.Parse(time.RFC3339Nano, l.Timestamp)
	out := events[:0]
	for _, e := range events {
		switch {
		// claude -p ends the run at its stop hooks, which the hook's notice already ended.
		case e.Kind == "result" && e.OK && t.hooked:
			continue
		case e.Kind != "notice":
			t.hooked = false
		}
		// A prompt the agent took mid-turn starts the reply that answers it.
		if t.start.IsZero() || e.Kind == "user" {
			t.start = at
		}
		if e.Kind == "result" {
			if (e.DurationMs == 0 || t.retime) && !t.start.IsZero() && !at.IsZero() {
				e.DurationMs = at.Sub(t.start).Milliseconds()
			}
			if l.IsAPIError {
				start := t.start
				t.erred = &start
			}
			t.start, t.retime = time.Time{}, false
		}
		out = append(out, e)
	}
	if hook {
		t.hooked, t.retime = true, true
	}
	return out
}

// follow keeps track of the agents running in the background, which Claude can stop all at once.
func (t *Turns) follow(l line, events []timeline.Event) {
	for _, e := range events {
		switch e.Kind {
		case "tool_background":
			if _, agent, _ := l.background(); agent {
				if t.agents == nil {
					t.agents, t.launched = map[string]string{}, map[string]string{}
				}
				t.agents[e.TaskID] = e.ToolUseID
				t.launched[e.TaskID] = e.ToolUseID
			}
		case "tool_resumed":
			if call, ok := t.launched[e.TaskID]; ok {
				t.agents[e.TaskID] = call
			}
		case "tool_end":
			delete(t.agents, e.TaskID)
			for task, call := range t.agents {
				if call == e.ToolUseID {
					delete(t.agents, task)
				}
			}
		}
	}
}

// killed ends the background agents' calls when Claude stops them all, as the user asked; it names none of them.
func (t *Turns) killed() []timeline.Event {
	var events []timeline.Event
	for _, call := range slices.Sorted(maps.Values(t.agents)) {
		events = append(events, timeline.Event{Kind: "tool_end", ToolUseID: call, OK: true})
	}
	return events
}

func (l line) events() (events []timeline.Event) {
	if e, ok := l.handback(); ok {
		return []timeline.Event{e}
	}
	if l.IsMeta && l.Type == "user" {
		if cause := l.cause(); cause != "" {
			return []timeline.Event{{Kind: "notice", Text: cause}}
		}
		return pastedImages(l.Message.Content)
	}
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
		// A task notification or prompt that comes mid-turn joins the turn as a queued command, not a user line.
		if l.Attachment.Type == "queued_command" {
			switch l.Attachment.CommandMode {
			case "task-notification":
				return taskEnd(l.Attachment.Prompt)
			case "prompt":
				// As on a user line, only a human's is a prompt; a sub-agent's hand-back or a goal's continuation is not.
				switch l.Attachment.Origin.Kind {
				case "", "human":
					return userEvents(l.Attachment.Prompt)
				case "auto-continuation":
					return notice(textOf(l.Attachment.Prompt))
				}
			}
		}
	case "assistant":
		// A failed request ends the turn with its error; claude -p writes no Stop after it.
		if l.IsAPIError {
			var text string
			for _, b := range blocks(l.Message.Content) {
				text += b.Text
			}
			return []timeline.Event{{Kind: "result", OK: false, Error: text, Limit: l.Error == "rate_limit"}}
		}
		// Claude writes a reply of its own where the model gave none, as "No response requested." after an interrupt.
		if l.Message.Model == "<synthetic>" {
			return nil
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
		case "auto-continuation":
			return notice(textOf(l.Message.Content))
		default:
			return nil
		}
		// A background call returns as it launches; it ends with its task notification.
		if task, _, ok := l.background(); ok {
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
		if task := l.resumed(); task != "" {
			events = append(events, timeline.Event{Kind: "tool_resumed", TaskID: task})
		}
		// Claude writes no notification for a command or workflow the agent stopped, so the stop's result ends it.
		if task, ok := l.stopped(); ok {
			out := "Stopped"
			events = append(events, timeline.Event{Kind: "tool_end", TaskID: task, OK: true, Output: &out})
		}
		return events
	}
	return events
}

// cause is why Claude goes on without a prompt, from a line the harness writes to set it going, or "".
func (l line) cause() string {
	text := textOf(l.Message.Content)
	switch {
	case l.TurnOrigin == "scheduled":
		return "Scheduled: " + firstLine(text)
	case strings.HasPrefix(text, "Stop hook feedback:"):
		// Claude names the hook, or the goal it checks, before the reason it gives.
		reason := strings.TrimSpace(strings.TrimPrefix(text, "Stop hook feedback:"))
		if _, after, ok := strings.Cut(reason, "]: "); ok && strings.HasPrefix(reason, "[") {
			reason = after
		}
		return "Stop hook: " + firstLine(reason)
	case strings.HasPrefix(text, "Goal check-in:"):
		return firstLine(text)
	case strings.HasPrefix(text, "Your claude.ai usage limit has reset."):
		return "Usage limit reset"
	}
	return ""
}

func notice(text string) []timeline.Event {
	if text = firstLine(text); text == "" {
		return nil
	}
	return []timeline.Event{{Kind: "notice", Text: text}}
}

func firstLine(text string) string {
	line, _, _ := strings.Cut(strings.TrimSpace(text), "\n")
	return line
}

// localOutput is what a command Claude ran itself printed.
func (l line) localOutput() (string, bool) {
	var text string
	if l.Type != "system" || l.Subtype != "local_command" || json.Unmarshal(l.Content, &text) != nil {
		return "", false
	}
	if out := tag(text, "local-command-stdout"); out != "" {
		return out, true
	}
	return tag(text, "local-command-stderr"), true
}

func (l line) caveat() bool {
	var text string
	return l.IsMeta && json.Unmarshal(l.Message.Content, &text) == nil && strings.HasPrefix(text, "<local-command-caveat>")
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

// background says whether the call went on in the background, an agent launched async, a command or a monitor, and its task id.
func (l line) background() (task string, agent, ok bool) {
	var r struct {
		IsAsync          bool   `json:"isAsync"`
		AgentID          string `json:"agentId"`
		BackgroundTaskID string `json:"backgroundTaskId"`
		TaskID           string `json:"taskId"`
		TimeoutMs        *int64 `json:"timeoutMs"`
	}
	switch {
	case json.Unmarshal(l.ToolUseResult, &r) != nil:
		return "", false, false
	case r.IsAsync:
		return r.AgentID, true, true
	// Other tools' results name a task too; only a monitor's has a timeout.
	case r.TimeoutMs != nil:
		return r.TaskID, false, r.TaskID != ""
	}
	return r.BackgroundTaskID, false, r.BackgroundTaskID != ""
}

// resumed is the agent a SendMessage set going again, or "".
func (l line) resumed() string {
	var r struct {
		ResumedAgentID string `json:"resumedAgentId"`
	}
	json.Unmarshal(l.ToolUseResult, &r)
	return r.ResumedAgentID
}

// handback ends a background agent's call with the report it hands back; its notification only points here.
func (l line) handback() (timeline.Event, bool) {
	o := l.Origin
	if l.Type == "attachment" {
		o = l.Attachment.Origin
	}
	if o.Kind != "peer" || !o.Handback || o.SenderTaskID == "" {
		return timeline.Event{}, false
	}
	out := o.HandbackReportText
	if out == "" {
		out = report(o.Body)
	}
	return timeline.Event{Kind: "tool_end", TaskID: o.SenderTaskID, OK: true, Output: &out}, true
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

// taskEnd ends the tool calls background tasks ran under, from the notification Claude writes when they stop.
func taskEnd(content json.RawMessage) []timeline.Event {
	var text string
	json.Unmarshal(content, &text)
	// The result is the agent's own text, which may hold any tag.
	head, _, _ := strings.Cut(text, "<result>")
	summary := html.UnescapeString(tag(head, "summary"))
	id, status, event := tag(head, "tool-use-id"), tag(head, "status"), tag(head, "event")
	// An agent that stopped with background work of its own still running resumes when it ends; what it said so far may be all it reports.
	if strings.HasSuffix(summary, "is waiting on its own background work") || strings.Contains(tag(head, "note"), "background work of its own still running") {
		e := timeline.Event{Kind: "tool_background", ToolUseID: id, Output: taskOutput(text, summary)}
		return each(e, head)
	}
	expired := strings.HasPrefix(event, "[Monitor timed out") || strings.HasPrefix(event, "[Monitor expired")
	// A monitor's other events, and a goal check-in, end nothing; they only set Claude going.
	if id == "" && status == "" && !expired {
		if desc, ok := strings.CutPrefix(summary, "Monitor event: "); ok {
			desc = "Monitor: " + strings.Trim(desc, `"`)
			if event != "" {
				desc += " · " + event
			}
			return notice(desc)
		}
		return notice(summary)
	}
	// A task the agent stopped ended as asked, not in error.
	e := timeline.Event{Kind: "tool_end", ToolUseID: id, OK: status != "failed", Output: taskOutput(text, summary)}
	if expired {
		e.Output = &event
	}
	return each(e, head)
}

// each is e for the call a notification names, or for each task it names when it leaves the call out, as Claude does
// for an agent's later run and for the tasks a previous session left behind.
func each(e timeline.Event, head string) []timeline.Event {
	if e.ToolUseID != "" {
		return []timeline.Event{e}
	}
	var events []timeline.Event
	for _, task := range tags(head, "task-id") {
		if !strings.HasPrefix(task, "__orphan_summary__") {
			e.TaskID = task
			events = append(events, e)
		}
	}
	return events
}

// taskOutput is an agent's result, or what became of a command and the end of its output.
func taskOutput(text, summary string) *string {
	if _, rest, ok := strings.Cut(text, "<result>"); ok {
		if end := strings.LastIndex(rest, "</result>"); end >= 0 {
			out := html.UnescapeString(strings.TrimSpace(rest[:end]))
			// The agent hands its report back in a line of its own.
			if strings.Contains(out, "report was delivered to you as a message from") || strings.HasPrefix(out, "This agent has not reported yet") {
				return nil
			}
			return &out
		}
	}
	out := strings.TrimSpace(summary + "\n\n" + tail(tag(text, "output-file")))
	if out == "" {
		return nil
	}
	return &out
}

// tail is the end of a command's output file. An agent's output file is a symlink to its transcript, which is not output, so it reads as "".
func tail(path string) string {
	const most = 2 << 10
	fi, err := os.Lstat(path)
	if err != nil || !fi.Mode().IsRegular() {
		return ""
	}
	f, err := os.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()
	from := max(fi.Size()-most, 0)
	b := make([]byte, fi.Size()-from)
	n, _ := f.ReadAt(b, from)
	b = b[:n]
	if from > 0 {
		_, b, _ = bytes.Cut(b, []byte("\n"))
	}
	return strings.TrimSpace(string(b))
}

func tag(text, name string) string {
	_, rest, ok := strings.Cut(text, "<"+name+">")
	value, _, closed := strings.Cut(rest, "</"+name+">")
	if !ok || !closed {
		return ""
	}
	return strings.TrimSpace(value)
}

func tags(text, name string) []string {
	var values []string
	for {
		value := tag(text, name)
		_, rest, ok := strings.Cut(text, "</"+name+">")
		if !ok {
			return values
		}
		values, text = append(values, value), rest
	}
}

var trailer = regexp.MustCompile(`\s*agentId: \S+ \(use SendMessage[^\n]*\)\s*(<usage>[\s\S]*</usage>)?\s*$`)

// report is a sub-agent's report without the harness's hand-back preamble and the trailer naming the agent.
func report(text string) string {
	text = trailer.ReplaceAllString(text, "")
	if _, rest, ok := strings.Cut(text, "The report follows:\n"); ok && strings.HasPrefix(text, "[Subagent hand-back]") {
		lines := strings.Split(rest, "\n")
		for i, l := range lines {
			lines[i] = strings.TrimPrefix(l, "  ")
		}
		text = strings.Join(lines, "\n")
	}
	return text
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
				out := report(textOf(b.Content))
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
	return append(events, timeline.Event{Kind: "user", Text: unwrapPastes(text)})
}

// unwrapPastes keeps the text of each block pasted into Claude's input, which it writes between
// <pasted_content id="…"> and </pasted_content id="…">.
func unwrapPastes(text string) string {
	const open = `<pasted_content id="`
	var out strings.Builder
	for {
		before, rest, ok := strings.Cut(text, open)
		out.WriteString(before)
		if !ok {
			return out.String()
		}
		id, inner, _ := strings.Cut(rest, `">`)
		body, after, closed := strings.Cut(inner, `</pasted_content id="`+id+`">`)
		if !closed {
			out.WriteString(open)
			text = rest
			continue
		}
		out.WriteString(strings.TrimSuffix(strings.TrimPrefix(body, "\n"), "\n"))
		text = after
	}
}

// pastedImages reads the line Claude writes after a prompt with pasted images, naming the file it saved each to,
// as the list the desktop attaches images to a prompt with.
func pastedImages(content json.RawMessage) []timeline.Event {
	var list []string
	for _, b := range blocks(content) {
		path, ok := strings.CutPrefix(b.Text, "[Image: source: ")
		path, closed := strings.CutSuffix(path, "]")
		if !ok || !closed || b.Type != "text" {
			return nil
		}
		list = append(list, "- "+path)
	}
	if len(list) == 0 {
		return nil
	}
	return []timeline.Event{{Kind: "user_images", Text: "# Attached images\n\n" + strings.Join(list, "\n")}}
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
	var texts []string
	for _, b := range blocks(raw) {
		if b.Text != "" {
			texts = append(texts, b.Text)
		}
	}
	return strings.Join(texts, "\n\n")
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
