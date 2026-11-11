package acp

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"time"

	"pocketd/internal/timeline"
)

// DelegationPoll is how often the transcripts of Hermes's sub-agents are read.
var DelegationPoll = time.Second

// watch shows what the sub-agents Hermes runs in the background for call
// write to their transcripts, the one place it tells of them, and ends the
// call once their manifest says they all have.
func (s *Session) watch(call string, transcripts []string) {
	d := &delegation{call: call, transcripts: transcripts, read: make([]int64, len(transcripts)), open: map[string][]string{}, summaries: map[int]string{},
		restored: s.opening, orphaned: s.opening && !s.carried}
	go func() {
		tick := time.NewTicker(DelegationPoll)
		defer tick.Stop()
		for {
			done, ok := d.ended()
			events := d.next()
			if done {
				events = append(events, d.end(ok))
			}
			s.mu.Lock()
			for _, e := range events {
				s.record(e)
			}
			s.mu.Unlock()
			if done {
				return
			}
			select {
			case <-s.ctx.Done():
				return
			case <-tick.C:
			}
		}
	}()
}

// delegation is what one delegate_task call's transcripts told so far.
type delegation struct {
	call        string
	transcripts []string
	read        []int64             // how far each transcript is read
	open        map[string][]string // the calls each task's tool started that haven't ended, oldest first, by task and tool
	calls       int
	summaries   map[int]string // what each task's sub-agent handed back
	// restored is one the history names, whose transcripts Hermes may have pruned since.
	restored bool
	// orphaned is one whose sub-agents ran in a process gone since, so nothing writes to its transcripts again.
	orphaned bool
}

// ended reads the manifest beside the transcripts: whether every sub-agent
// has ended, and whether each completed. A manifest gone ends the call
// failed, unless the history names it: Hermes prunes transcripts after a
// week, so that one ends as the call did.
func (d *delegation) ended() (done, ok bool) {
	b, err := os.ReadFile(filepath.Join(filepath.Dir(d.transcripts[0]), "manifest.json"))
	if errors.Is(err, fs.ErrNotExist) {
		return true, d.restored
	}
	var m struct {
		Completed string `json:"completed"`
		Tasks     []struct {
			Status string `json:"status"`
		} `json:"tasks"`
	}
	// Hermes rewrites it in place, so a read may catch it half written.
	if err != nil || json.Unmarshal(b, &m) != nil {
		return d.orphaned, false
	}
	// With independent completions, each unit marks it completed as it ends.
	done, ok = m.Completed != "", true
	for _, t := range m.Tasks {
		done = done && t.Status != "running"
		ok = ok && t.Status == "completed"
	}
	if !done {
		return d.orphaned, false
	}
	return true, ok
}

// next is what the transcripts' lines written since the last read tell.
func (d *delegation) next() []timeline.Event {
	var events []timeline.Event
	for i, path := range d.transcripts {
		f, err := os.Open(path)
		if err != nil {
			continue
		}
		f.Seek(d.read[i], io.SeekStart)
		b, _ := io.ReadAll(f)
		f.Close()
		// A line still being written waits for the next read.
		whole := b[:strings.LastIndexByte(string(b), '\n')+1]
		d.read[i] += int64(len(whole))
		for line := range strings.SplitSeq(strings.TrimSuffix(string(whole), "\n"), "\n") {
			if e, ok := d.line(i, line); ok {
				events = append(events, e)
			}
		}
	}
	return events
}

// line reads one "15:04:05 role     | text" line of task's transcript. Its
// header, its kickoff, which the call's brief shows, and its markers tell nothing.
func (d *delegation) line(task int, line string) (timeline.Event, bool) {
	if len(line) < 9 || line[2] != ':' || line[5] != ':' {
		return timeline.Event{}, false
	}
	role, text, ok := strings.Cut(line[9:], "| ")
	if !ok {
		return timeline.Event{}, false
	}
	switch strings.TrimSpace(role) {
	case "assistant":
		return timeline.Event{Kind: "assistant_text", Text: text, ParentToolUseID: d.call}, true
	case "think":
		return timeline.Event{Kind: "thinking", Text: text, ParentToolUseID: d.call}, true
	case "tool":
		name, args, _ := strings.Cut(strings.TrimPrefix(text, "-> "), "(")
		d.calls++
		id := fmt.Sprintf("%s.%d", d.call, d.calls)
		key := fmt.Sprint(task, name)
		d.open[key] = append(d.open[key], id)
		name, detail := hermesTool(name, strings.TrimSuffix(args, ")")).describe()
		return timeline.Event{Kind: "tool_start", ToolUseID: id, Name: name, Detail: &detail, ParentToolUseID: d.call}, true
	case "result":
		// "<tool> ok 1.2s: <output>", or ERROR.
		head, out, _ := strings.Cut(text, ": ")
		fields := strings.Fields(head)
		if len(fields) < 2 {
			return timeline.Event{}, false
		}
		key := fmt.Sprint(task, fields[0])
		if len(d.open[key]) == 0 {
			return timeline.Event{}, false
		}
		id := d.open[key][0]
		d.open[key] = d.open[key][1:]
		return timeline.Event{Kind: "tool_end", ToolUseID: id, OK: fields[1] == "ok", Output: &out}, true
	case "final":
		if _, summary, ok := strings.Cut(text, "summary: "); ok {
			d.summaries[task] = summary
		}
	}
	return timeline.Event{}, false
}

// hermesTool is a call a sub-agent's transcript names, with the preview of its arguments.
func hermesTool(name, args string) *toolCall {
	kinds := map[string]string{"terminal": "execute", "read_file": "read", "search_files": "search"}
	if kind := kinds[name]; kind != "" {
		return &toolCall{Kind: kind, Title: args}
	}
	return &toolCall{Title: strings.TrimSpace(name + " " + args)}
}

// end ends the call with what its sub-agents handed back.
func (d *delegation) end(ok bool) timeline.Event {
	var said []string
	for i := range d.transcripts {
		if s := d.summaries[i]; s != "" {
			said = append(said, s)
		}
	}
	e := timeline.Event{Kind: "tool_end", ToolUseID: d.call, OK: ok}
	if len(said) > 0 {
		out := strings.Join(said, "\n\n")
		e.Output = &out
	}
	return e
}
