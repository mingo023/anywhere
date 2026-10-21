package daemon

import (
	"context"
	"errors"
	"pocketd/internal/accounts"
	"slices"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"pocketd/internal/agent"
	"pocketd/internal/claude"
	"pocketd/internal/codex"
	"pocketd/internal/proc"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

// presence is the agent that process pid is, for as long as pid stays in t's foreground.
type presence struct {
	t         *terminal.Terminal
	a         *agent.Agent
	pid       int
	provider  string
	argv, env []string
	ctx       context.Context
	cancel    context.CancelFunc
	sock      string   // codex's app-server; set under Daemon.mu
	stream    *stream  // the stdio of an agent run for the chat; nil for one in its TUI
	chat      pipeChat // its chat behind `pocketd pipe`; nil in a TUI

	mu         sync.Mutex
	transcript string
	stopTail   func()
	enter      time.Time
	prompted   bool // codex started on a prompt: its first thread is its own
	thread     string
	unfollow   context.CancelFunc
	asks       map[string]int // open permission requests by permissionKey
	launch     state.Launch
	hooked     bool // a SessionStart came; a restored Agent has its Conversation before that
	reported   bool // an OSC 7501 report came: status is theirs, not the hooks'
	typed      bool // the user started it in a shell: Pocket never restarts it
	drafted    bool // Claude's input may hold text the user typed
	stashed    bool // Claude holds a draft put aside with Ctrl+S
	line       line // the text keys put in Claude's input since the last Enter
	aside      line // line as it was when Ctrl+S put it aside

	// sending guards sent apart from mu, as a tail notes what it shows while
	// mu waits for that tail to stop.
	sending sync.Mutex
	sent    []received // prompts Claude received that the transcript it follows hasn't shown
	typing  []string   // prompts typedPrompt noted, each until the Enter that sends it

	// report makes applying a report atomic with reading the terminal's latest
	// one, so a catch-up can't overwrite a live report that arrived meanwhile.
	report sync.Mutex
}

func (d *Daemon) startAgent(t *terminal.Terminal, provider string, p proc.Proc) *presence {
	ctx, cancel := context.WithCancel(context.Background())
	argv, s := p.Argv, (*stream)(nil)
	if inner := piped(argv); inner != nil {
		argv, s = inner, newStream()
	}
	pr := &presence{t: t, pid: p.Pid, provider: provider, argv: argv, env: p.Env, ctx: ctx, cancel: cancel, asks: map[string]int{},
		launch: state.Parse(provider, argv), stream: s}
	if s != nil {
		pr.chat = kindOf[provider].pipe(d, pr)
	}
	kindOf[provider].started(d, pr)
	info := t.Info()
	driver := func(a *agent.Agent) agent.Driver {
		c := termDriver{t: t, a: a, pid: p.Pid, pr: pr}
		if pr.chat != nil {
			return pr.chat.driver(c)
		}
		return c
	}
	origin := t.TakeOrigin()
	h := d.adopt(pr, driver)
	if h != nil {
		pr.a = h.a
	} else {
		pr.a = d.Agents.AddFunc(terminal.NewID(), info.Cwd, provider, driver)
	}
	// A handed-down Terminal has lost its origin; its hint still knows.
	pr.typed = origin == "" && (h == nil || h.saved.Typed)
	pr.a.SetTerminal(info.ID)
	if s != nil {
		pr.a.SetChatOnly()
	}
	pr.a.SetEffort(pr.launch.Effort)
	pr.a.SetAccess(pr.launch.Access)
	// Claude names its model only in its first reply; until then the one it was started with stands in.
	if pr.launch.Model != "" && pr.a.Summary().Model == "" {
		pr.a.SetModel(pr.launch.Model)
	}
	if d.Accounts != nil {
		pr.a.SetAccount(d.Accounts.Of(provider, p.Env))
	}
	if origin != "" {
		pr.a.SetOrigin(origin)
	}
	d.place(pr)
	d.mu.Lock()
	if d.present == nil {
		d.present = map[string]*presence{}
	}
	d.present[info.ID] = pr
	d.mu.Unlock()
	d.catchUp(pr)
	return pr
}

// Running counts the agents running on provider's account id.
func (d *Daemon) Running(provider, id string) int {
	if id == accounts.BuiltIn {
		id = ""
	}
	d.mu.Lock()
	var all []*presence
	for _, pr := range d.present {
		if pr.provider == provider {
			all = append(all, pr)
		}
	}
	d.mu.Unlock()
	n := 0
	for _, pr := range all {
		if pr.a.Summary().Account == id {
			n++
		}
	}
	return n
}

// place names the Project and Worktree pr's Terminal was opened in. The
// branch can move under a running agent, so each turn end places it again.
func (d *Daemon) place(pr *presence) {
	p, _ := worktree.Find(d.Registry.Load(), pr.t.Info().Cwd)
	pr.a.SetLocation(p.Project, p.Worktree, p.Branch, p.Main)
}

// cleared is a turn the user stopped: it ends like any other, so the agent may move.
func (d *Daemon) cleared(pr *presence) {
	_, seq := pr.a.Timeline.State()
	pr.a.Clear()
	go d.Follow(pr.provider)
	settled := time.After(StopSettle)
	go func() {
		select {
		case <-pr.ctx.Done():
		case <-settled:
			pr.a.EndStopped(seq)
		}
	}()
}

func (d *Daemon) turnEnded(pr *presence, failed bool) {
	pr.a.TurnEnded(failed)
	d.place(pr)
	go d.Follow(pr.provider)
}

// StopSettle is how long Claude has to mark a stop in its transcript.
var StopSettle = 2 * time.Second

// ClaudeAttachWait is how long a new claude has to send its SessionStart.
// Claude skips hooks in a folder the user hasn't trusted.
var ClaudeAttachWait = 5 * time.Second

// attach connects pr to its provider's status. A claude starts attached, so
// it doesn't flash "not attached" before its first hook.
func (d *Daemon) attach(pr *presence) {
	if h := d.hintOf(pr); h != nil && h.handoff {
		defer d.handedOver(pr, h)
	}
	if pr.chat != nil {
		pr.chat.attach()
		return
	}
	kindOf[pr.provider].attach(d, pr)
}

func (pr *presence) isDrafted() bool {
	pr.mu.Lock()
	defer pr.mu.Unlock()
	return pr.drafted
}

// typedKeys follows Claude's input through the keys written to it. It errs
// towards a draft: Ctrl+S on an empty input with nothing put aside does nothing.
// A paste, which a file dropped on the terminal is too, comes bracketed.
func (pr *presence) typedKeys(b []byte) {
	pr.mu.Lock()
	sent, enter := "", false
	switch s := string(b); {
	case s == "\r" || s == "\x1b[13u":
		if enter = true; !pr.line.lost {
			sent = pr.line.text
		}
		// Claude puts a stashed draft back once a prompt is sent.
		pr.drafted, pr.stashed = pr.stashed, false
		pr.line, pr.aside = pr.aside, line{}
		pr.a.Sent()
	case s == "\x13":
		pr.drafted, pr.stashed = pr.stashed, pr.drafted
		pr.line, pr.aside = pr.aside, pr.line
	default:
		pr.line = pr.line.typed(s)
		if b[0] != 0x1b && printable(s) || strings.HasPrefix(s, "\x1b[200~") && printable(strings.TrimSuffix(s[len("\x1b[200~"):], "\x1b[201~")) {
			pr.drafted = true
		}
	}
	pr.mu.Unlock()
	if enter {
		pr.entered(sent)
	}
}

func printable(s string) bool {
	return strings.ContainsFunc(s, func(r rune) bool { return r >= 0x20 && r != 0x7f })
}

// line is the text keys put in Claude's input, as far as they show it:
// once an edit it can't follow comes, like an arrow key or Ctrl+U, it's lost.
// Ctrl+C empties the input, and Shift+Tab, or Esc on an empty one, leaves it.
type line struct {
	text string
	lost bool
}

func (l line) typed(s string) line {
	if paste, ok := strings.CutPrefix(s, "\x1b[200~"); ok {
		l.text += strings.TrimSuffix(paste, "\x1b[201~")
		return l
	}
	switch {
	case s == "\x1b\r" || s == "\x1b[13;2u":
		l.text += "\n"
		return l
	case s == "\x03" || s == "\x1b[99;5u":
		return line{}
	case s == "\x1b[Z" || (s == "\x1b" || s == "\x1b[27u") && l == line{}:
		return l
	}
	if strings.HasPrefix(s, "\x1b") {
		l.lost = true
		return l
	}
	for _, r := range s {
		switch {
		case r == 0x7f || r == '\b':
			_, n := utf8.DecodeLastRuneInString(l.text)
			l.text = l.text[:len(l.text)-n]
		case r < 0x20:
			l.lost = true
		default:
			l.text += string(r)
		}
	}
	return l
}

// typedPrompt notes a prompt pocketd types into Claude.
func (pr *presence) typedPrompt(text string) {
	pr.sending.Lock()
	defer pr.sending.Unlock()
	pr.typing = append(pr.typing, text)
	pr.received(text)
}

// entered notes a prompt Claude got other than through typedPrompt: one its
// launch gave it, or a line sent from its input, "" when keys lost it. The
// Enter after a prompt pocketd types comes through here too.
func (pr *presence) entered(text string) {
	pr.sending.Lock()
	defer pr.sending.Unlock()
	if len(pr.typing) > 0 {
		typing := pr.typing[0]
		if pr.typing = pr.typing[1:]; tidy(typing) == tidy(text) {
			return
		}
	}
	pr.received(text)
}

// unshownMax is how many prompts no transcript showed are kept: lines that
// answered Claude's dialogs never show, nor do any while no transcript is followed.
const unshownMax = 16

// received is a prompt Claude got, and when: Claude writes it to a
// transcript after, so a transcript up to it from before isn't its.
type received struct {
	text string
	at   time.Time
}

// received notes a prompt Claude got. A /clear starts a conversation the
// prompts before it can't open.
func (pr *presence) received(text string) {
	switch {
	case tidy(text) == "/clear":
		pr.sent = nil
	case claude.Command(text) || tidy(text) == "":
	default:
		pr.sent = append(pr.sent, received{text, time.Now()})
		if len(pr.sent) > unshownMax {
			pr.sent = slices.Delete(pr.sent, 0, len(pr.sent)-unshownMax)
		}
	}
}

// shown drops the prompts typed up to the one the transcript pr follows shows.
func (pr *presence) shown(echo string) {
	pr.sending.Lock()
	defer pr.sending.Unlock()
	if i := slices.IndexFunc(pr.sent, func(r received) bool { return sameText(echo, r.text) }); i >= 0 {
		pr.sent = pr.sent[i+1:]
	}
}

func (pr *presence) unshown() []received {
	pr.sending.Lock()
	defer pr.sending.Unlock()
	return slices.Clone(pr.sent)
}

func (pr *presence) isReported() bool {
	pr.mu.Lock()
	defer pr.mu.Unlock()
	return pr.reported
}

// working is Working unless a permission request other than key's is open:
// a parallel tool's hooks must not hide that dialog. Key's own tool ending
// means its dialog was answered at the desk.
func (pr *presence) working(key string) {
	pr.mu.Lock()
	defer pr.mu.Unlock()
	if len(pr.asks) == 0 || len(pr.asks) == 1 && pr.asks[key] > 0 {
		pr.a.Working()
	}
}

func (pr *presence) setMode(permissionMode string) {
	pr.mu.Lock()
	pr.launch = state.Mode(pr.launch, permissionMode)
	access := pr.launch.Access
	pr.mu.Unlock()
	pr.a.SetAccess(access)
}

// accessSwitched leaves plan mode, as claude does on a switch.
func (pr *presence) accessSwitched(access string) {
	pr.mu.Lock()
	pr.launch.Access, pr.launch.Plan = access, false
	pr.mu.Unlock()
	pr.a.SetAccess(access)
}

// save puts pr's Conversation in e, once there is one to resume.
func (pr *presence) save(e *state.Terminal) {
	s := pr.a.Summary()
	if s.ProviderSessionID == "" {
		return
	}
	pr.mu.Lock()
	launch := pr.launch
	e.TranscriptPath = pr.transcript
	pr.mu.Unlock()
	e.Provider, e.ConversationID, e.Launch = pr.provider, s.ProviderSessionID, &launch
	e.AgentID, e.CreatedAt, e.Status, e.Failed, e.Origin, e.Pinned = s.ID, s.CreatedAt, s.Status, s.Failed, s.Origin, s.Pinned
	e.Account, e.Typed, e.Chat = s.Account, pr.typed, pr.stream != nil
}

// endAgent ends pr once; the poller and a hook's observe can both find it gone.
func (d *Daemon) endAgent(pr *presence) {
	id := pr.t.Info().ID
	d.mu.Lock()
	if d.present[id] != pr {
		d.mu.Unlock()
		return
	}
	delete(d.present, id)
	d.unwatch(pr.sock)
	h := d.restoring[id]
	moving := h != nil && h.moved && h.pr == nil && h.a == pr.a
	if h != nil && h.pr == pr {
		h.pr = nil
	} else {
		h = nil
	}
	d.mu.Unlock()
	pr.cancel()
	if !moving {
		pr.t.ClearProgram()
	}
	pr.mu.Lock()
	if pr.stopTail != nil {
		pr.stopTail()
	}
	if pr.unfollow != nil {
		pr.unfollow()
	}
	pr.mu.Unlock()
	if moving {
		return
	}
	d.Broker.DenyAll(pr.a.ID())
	if h == nil {
		d.Agents.Remove(pr.a.ID())
		return
	}
	d.finish(h, "agent_exited")
}

type termDriver struct {
	t   *terminal.Terminal
	a   *agent.Agent
	pid int
	pr  *presence
}

var errNotForeground = errors.New("Agent is no longer in the foreground")

// errAgentsRunOn refuses a stop between turns: Esc would stop no sub-agent.
var errAgentsRunOn = errors.New("Claude has no turn to stop; its background agents keep running")

// Prompt, Interrupt and Compact refuse once pid left the foreground: the
// poller may not have noticed yet, and whatever they type would reach the shell.
func (c termDriver) Prompt(text string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	return c.typeIn(text)
}

func (c termDriver) typeIn(text string) error {
	return kindOf[c.pr.provider].typeIn(c, text)
}

func (c termDriver) Interrupt() error {
	if !c.inForeground() {
		return errNotForeground
	}
	if !c.a.Stop() {
		if c.a.Timeline.BackgroundAgents() > 0 {
			return errAgentsRunOn
		}
		return nil
	}
	return c.t.Write([]byte{0x1b})
}

// Close leaves a pid that left the foreground alone: it may belong to another process by now.
func (c termDriver) Close() {
	if c.inForeground() {
		proc.Terminate(c.pid)
	}
}

func (c termDriver) Compact() error {
	if !c.inForeground() {
		return errNotForeground
	}
	c.a.SetCompacting()
	return c.typeIn("/compact")
}

// Settings switches through the provider; a resume keeps the switch.
func (c termDriver) Settings(model, effort string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	if err := kindOf[c.pr.provider].settings(c, model, effort); err != nil {
		return err
	}
	c.pr.switched(model, effort)
	return nil
}

var errTUIAccess = errors.New("Switch this agent's access in its terminal")

func (c termDriver) Access(string) error { return errTUIAccess }

// Commands is none: a TUI agent has its own menu.
func (c termDriver) Commands() ([]proto.Command, error) { return nil, nil }

// typeSettings waits out the Enter that Prompt sends late, so the two
// commands don't run together.
func (c termDriver) typeSettings(model, effort string) error {
	if model != "" {
		if err := c.typeIn("/model " + model); err != nil || effort == "" {
			return err
		}
		time.Sleep(300 * time.Millisecond)
	}
	return c.typeIn("/effort " + effort)
}

var errNoThread = errors.New("Codex can switch once its first turn has started")

// switchCodex needs pr's thread, which only a codex on its account's
// app-server gets: one bound has its sock set.
func (pr *presence) switchCodex(model, effort string) error {
	pr.mu.Lock()
	thread := pr.thread
	pr.mu.Unlock()
	if thread == "" {
		return errNoThread
	}
	ctx, cancel := context.WithTimeout(pr.ctx, 5*time.Second)
	defer cancel()
	return codex.UpdateSettings(ctx, pr.sock, thread, model, effort)
}

func (pr *presence) switched(model, effort string) {
	pr.mu.Lock()
	if model != "" {
		pr.launch.Model = model
	}
	if effort != "" {
		pr.launch.Effort = effort
	}
	pr.mu.Unlock()
	if model != "" {
		pr.a.SetModel(model)
	}
	if effort != "" {
		pr.a.SetEffort(effort)
	}
}

func (c termDriver) inForeground() bool {
	_, procs := foreground(c.t)
	return slices.ContainsFunc(procs, func(p proc.Proc) bool { return p.Pid == c.pid })
}
