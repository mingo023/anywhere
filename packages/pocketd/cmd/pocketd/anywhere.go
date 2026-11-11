package main

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"math/rand/v2"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"text/tabwriter"
	"time"

	"pocketd/internal/cliskill"
	"pocketd/internal/config"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
)

// anywhereUsage names the agents a session runs as: its chat is how the
// command prompts and reads it, so only those pocketd drives a chat of.
func anywhereUsage() string {
	return `usage: anywhere project list [--json]
       anywhere session list [--json]
       anywhere session read [--limit 200] [--json] <id>
       anywhere session create [--project <name|path>] [--worktree <name> | --new-worktree [<name>] [--base <branch>]] [--provider <agent>] [--access settings|ask|edits|auto|full] [--prompt <text>]
       anywhere session prompt <id> <text|->
       anywhere session close <id>
       anywhere session wait [--timeout 30m] <id>
       anywhere ticket create [--project <name|path>] [--provider <agent>] [--base <branch>] [--title <text>] [--start] <prompt|->
       anywhere ticket list [--json]
       anywhere ticket read [--json] <id>
       anywhere ticket update [--title <text>] [--provider <agent>] [--base <branch>] [--prompt <text|->] <id>
       anywhere ticket delete <id>
<agent> is ` + strings.Join(providers.Chatting(), "|")
}

const (
	exitError    = 1
	exitUsage    = 2
	exitFailed   = 3
	exitNeedsYou = 4
	exitTimeout  = 5
	exitClosed   = 6
)

// turnStart is how long a prompted agent has to start its turn.
const turnStart = 30 * time.Second

// settleFor outlasts a few of claude.Tail's 100ms polls.
const settleFor = 500 * time.Millisecond

var errUsage = errors.New("usage")

func anywhereMain(args []string, stdout, stderr io.Writer) int {
	code, err := anywhere(args, stdout)
	switch {
	case errors.Is(err, errUsage):
		fmt.Fprintln(stderr, anywhereUsage())
		return exitUsage
	case err != nil:
		fmt.Fprintln(stderr, "anywhere:", err)
		return exitError
	}
	return code
}

func anywhere(args []string, stdout io.Writer) (int, error) {
	if len(args) < 2 {
		return 0, errUsage
	}
	flags := flag.NewFlagSet("anywhere", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	rest := args[2:]
	switch args[0] + " " + args[1] {
	case "project list":
		return 0, projectListCmd(flags, rest, stdout)
	case "session list":
		return 0, sessionListCmd(flags, rest, stdout)
	case "session read":
		return 0, sessionReadCmd(flags, rest, stdout)
	case "session create":
		return 0, sessionCreateCmd(flags, rest, stdout)
	case "session prompt":
		return 0, sessionPromptCmd(flags, rest)
	case "session close":
		return 0, sessionCloseCmd(flags, rest)
	case "session wait":
		return sessionWaitCmd(flags, rest, stdout)
	case "ticket create":
		return 0, ticketCreateCmd(flags, rest, stdout)
	case "ticket list":
		return 0, ticketListCmd(flags, rest, stdout)
	case "ticket read":
		return 0, ticketReadCmd(flags, rest, stdout)
	case "ticket update":
		return 0, ticketUpdateCmd(flags, rest)
	case "ticket delete":
		return 0, ticketDeleteCmd(flags, rest)
	}
	return 0, errUsage
}

// positional parses args with flags before or after the positional arguments,
// which it returns.
func positional(flags *flag.FlagSet, args []string) ([]string, error) {
	var out []string
	for {
		if err := flags.Parse(args); err != nil {
			return nil, errUsage
		}
		if flags.NArg() == 0 {
			return out, nil
		}
		out = append(out, flags.Arg(0))
		args = flags.Args()[1:]
	}
}

func printJSON(w io.Writer, v any) error {
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	return enc.Encode(v)
}

func projects(c *client) ([]proto.Project, error) {
	raw, err := c.call("project.list", map[string]any{"type": "project.list"})
	if err != nil {
		return nil, err
	}
	var l proto.ProjectList
	if err := json.Unmarshal(raw, &l); err != nil {
		return nil, err
	}
	return l.Projects, nil
}

func projectListCmd(flags *flag.FlagSet, args []string, w io.Writer) error {
	asJSON := flags.Bool("json", false, "")
	if pos, err := positional(flags, args); err != nil || len(pos) != 0 {
		return errUsage
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return err
	}
	defer c.close()
	list, err := projects(c)
	if err != nil {
		return err
	}
	if *asJSON {
		return printJSON(w, list)
	}
	tw := tabwriter.NewWriter(w, 0, 4, 2, ' ', 0)
	for _, p := range list {
		fmt.Fprintf(tw, "%s\t%s\n", p.Name, p.Path)
	}
	return tw.Flush()
}

func sessionListCmd(flags *flag.FlagSet, args []string, w io.Writer) error {
	asJSON := flags.Bool("json", false, "")
	if pos, err := positional(flags, args); err != nil || len(pos) != 0 {
		return errUsage
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return err
	}
	defer c.close()
	list := make([]proto.AgentSummary, 0, len(c.agents))
	for _, a := range c.agents {
		list = append(list, a)
	}
	slices.SortFunc(list, func(a, b proto.AgentSummary) int { return cmp.Compare(b.UpdatedAt, a.UpdatedAt) })
	if *asJSON {
		return printJSON(w, list)
	}
	tw := tabwriter.NewWriter(w, 0, 4, 2, ' ', 0)
	for _, a := range list {
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\n", a.ID, a.Status, a.Provider, a.Project, a.Title)
	}
	return tw.Flush()
}

type timelineItem struct {
	Kind string `json:"kind"`
	Text string `json:"text"`
}

// lastReply is what the assistant said after the last user message.
func lastReply(items []timelineItem) string {
	var reply []string
	for _, it := range items {
		switch it.Kind {
		case "user":
			reply = nil
		case "assistant":
			reply = append(reply, it.Text)
		}
	}
	return strings.Join(reply, "\n\n")
}

func sessionReadCmd(flags *flag.FlagSet, args []string, w io.Writer) error {
	asJSON := flags.Bool("json", false, "")
	limit := flags.Int("limit", 200, "")
	pos, err := positional(flags, args)
	if err != nil || len(pos) != 1 || *limit < 1 || *limit > 500 {
		return errUsage
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return err
	}
	defer c.close()
	raw, err := c.call("agent.timeline", map[string]any{"type": "agent.timeline", "agentId": pos[0], "limit": *limit})
	if err != nil {
		return err
	}
	var page struct {
		Items []json.RawMessage `json:"items"`
	}
	if err := json.Unmarshal(raw, &page); err != nil {
		return err
	}
	if *asJSON {
		return printJSON(w, page.Items)
	}
	var items []timelineItem
	for _, r := range page.Items {
		var it timelineItem
		if err := json.Unmarshal(r, &it); err != nil {
			return err
		}
		items = append(items, it)
	}
	_, err = fmt.Fprintln(w, lastReply(items))
	return err
}

type newSession struct {
	project, worktree, base, provider, access, prompt string
	newWorktree                                       freshWorktree
}

// freshWorktree is --new-worktree, whose name is optional: without one, the Worktree
// gets a placeholder and pocketd names its branch from the prompt, as for the desktop's.
type freshWorktree struct {
	wanted bool
	name   string
}

func (f *freshWorktree) String() string   { return f.name }
func (f *freshWorktree) IsBoolFlag() bool { return true }

// Set gets "true" from a bare --new-worktree, and the name from --new-worktree=<name>.
func (f *freshWorktree) Set(s string) error {
	if s == "" {
		return errUsage
	}
	f.wanted = true
	if s != "true" {
		f.name = s
	}
	return nil
}

func inside(dir, root string) bool {
	return dir == root || strings.HasPrefix(dir, root+string(filepath.Separator))
}

// pickProject is the Project named by arg, a name or a path, or the one holding cwd.
func pickProject(list []proto.Project, arg, cwd string) (proto.Project, error) {
	if arg == "" {
		for _, p := range list {
			for _, w := range p.Worktrees {
				if inside(cwd, w.Path) {
					return p, nil
				}
			}
		}
		return proto.Project{}, fmt.Errorf("%s isn't in a project; pass --project", cwd)
	}
	path, _ := filepath.Abs(arg)
	var named []proto.Project
	for _, p := range list {
		if p.Path == path {
			return p, nil
		}
		if p.Name == arg {
			named = append(named, p)
		}
	}
	switch len(named) {
	case 0:
		return proto.Project{}, fmt.Errorf("no project %q", arg)
	case 1:
		return named[0], nil
	}
	return proto.Project{}, fmt.Errorf("%d projects are named %q; pass its path", len(named), arg)
}

func pickWorktree(p proto.Project, name string) (proto.Worktree, error) {
	for _, w := range p.Worktrees {
		if name == "" && w.IsMain || name != "" && w.Name == name {
			return w, nil
		}
	}
	return proto.Worktree{}, fmt.Errorf("project %s has no worktree %q", p.Name, name)
}

func launchSpec(list []proto.Project, cwd string, n newSession) (proto.LaunchSpec, error) {
	p, err := pickProject(list, n.project, cwd)
	if err != nil {
		return proto.LaunchSpec{}, err
	}
	spec := proto.LaunchSpec{Project: p.Path, Provider: n.provider, Access: n.access, Prompt: n.prompt, Chat: true}
	if f := n.newWorktree; f.wanted {
		name := f.name
		if name == "" {
			name = fmt.Sprintf("cli-%08x", rand.Uint32())
		}
		spec.Checkout.New = &proto.NewWorktree{Name: name, Base: n.base, AutoName: f.name == "" && n.prompt != ""}
		return spec, nil
	}
	w, err := pickWorktree(p, n.worktree)
	if err != nil {
		return proto.LaunchSpec{}, err
	}
	spec.Checkout.Worktree = w.Path
	return spec, nil
}

func parseCreate(flags *flag.FlagSet, args []string) (newSession, error) {
	var n newSession
	flags.StringVar(&n.project, "project", "", "")
	flags.StringVar(&n.worktree, "worktree", "", "")
	flags.Var(&n.newWorktree, "new-worktree", "")
	flags.StringVar(&n.base, "base", "", "")
	flags.StringVar(&n.provider, "provider", "claude", "")
	flags.StringVar(&n.access, "access", "settings", "")
	flags.StringVar(&n.prompt, "prompt", "", "")
	pos, err := positional(flags, args)
	if err != nil {
		return n, errUsage
	}
	// flag leaves the name of `--new-worktree fix-login` to the positionals, since the name is optional.
	if f := &n.newWorktree; f.wanted && f.name == "" && len(pos) == 1 && pos[0] != "" {
		f.name, pos = pos[0], nil
	}
	blank := false
	flags.Visit(func(f *flag.Flag) {
		if (f.Name == "worktree" || f.Name == "base") && f.Value.String() == "" {
			blank = true
		}
	})
	if len(pos) != 0 || blank || n.worktree != "" && n.newWorktree.wanted || n.base != "" && !n.newWorktree.wanted {
		return n, errUsage
	}
	return n, nil
}

// openProjects connects to pocketd and returns its Projects and the folder to pick one from.
func openProjects() (*client, []proto.Project, string, error) {
	cwd, err := os.Getwd()
	if err != nil {
		return nil, nil, "", err
	}
	if cwd, err = filepath.EvalSymlinks(cwd); err != nil {
		return nil, nil, "", err
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return nil, nil, "", err
	}
	list, err := projects(c)
	if err != nil {
		c.close()
		return nil, nil, "", err
	}
	return c, list, cwd, nil
}

func sessionCreateCmd(flags *flag.FlagSet, args []string, w io.Writer) error {
	n, err := parseCreate(flags, args)
	if err != nil {
		return err
	}
	c, list, cwd, err := openProjects()
	if err != nil {
		return err
	}
	defer c.close()
	spec, err := launchSpec(list, cwd, n)
	if err != nil {
		return err
	}
	requestID := fmt.Sprintf("anywhere-%d-%d", os.Getpid(), time.Now().UnixNano())
	raw, err := c.call("agent.created", map[string]any{"type": "agent.create", "requestId": requestID, "spec": spec})
	if err != nil {
		return err
	}
	var created proto.AgentCreated
	if err := json.Unmarshal(raw, &created); err != nil {
		return err
	}
	// The session exists whether or not its turn starts, so the caller gets its id either way.
	if _, err := fmt.Fprintln(w, created.AgentID); err != nil || n.prompt == "" {
		return err
	}
	return c.awaitTurn(created.AgentID)
}

// awaitTurn returns once id has been seen in a turn, so a wait right after
// doesn't take the idle before the turn for its end.
func (c *client) awaitTurn(id string) error {
	ctx, cancel := context.WithTimeout(c.ctx, turnStart)
	defer cancel()
	for !c.began[id] {
		if c.agents[id].Status == "closed" {
			return fmt.Errorf("session %s closed before its turn started", id)
		}
		if _, _, _, err := c.read(ctx); err != nil {
			if ctx.Err() != nil {
				return fmt.Errorf("session %s didn't start a turn within %s", id, turnStart)
			}
			return err
		}
	}
	return nil
}

// promptText is arg, or standard input when arg is "-".
func promptText(arg string) (string, error) {
	if arg != "-" {
		return arg, nil
	}
	in, err := io.ReadAll(os.Stdin)
	return string(in), err
}

func sessionPromptCmd(flags *flag.FlagSet, args []string) error {
	pos, err := positional(flags, args)
	if err != nil || len(pos) != 2 {
		return errUsage
	}
	id := pos[0]
	text, err := promptText(pos[1])
	if err != nil {
		return err
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return err
	}
	defer c.close()
	a, ok := c.agents[id]
	if !ok {
		return fmt.Errorf("no session %s", id)
	}
	if a.Status != "working" && a.Status != "needsYou" {
		delete(c.began, id)
	}
	if _, err := c.call("ack", map[string]any{"type": "agent.prompt", "agentId": id, "text": text}); err != nil {
		return err
	}
	if !a.Attached {
		return nil
	}
	return c.awaitTurn(id)
}

func sessionCloseCmd(flags *flag.FlagSet, args []string) error {
	pos, err := positional(flags, args)
	if err != nil || len(pos) != 1 {
		return errUsage
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return err
	}
	defer c.close()
	_, err = c.call("ack", map[string]any{"type": "agent.close", "agentId": pos[0]})
	return err
}

// outcome is what wait reports about a, and false while a is still working.
func outcome(a proto.AgentSummary) (code int, word string, over bool) {
	switch {
	case a.Status == "working":
		return 0, "", false
	case a.Status == "needsYou":
		return exitNeedsYou, "needsYou", true
	case a.Status == "closed":
		return exitClosed, "closed", true
	case a.Failed:
		return exitFailed, "failed", true
	}
	// A turn that ends while the session is on screen goes to idle, not done.
	return 0, "done", true
}

func sessionWaitCmd(flags *flag.FlagSet, args []string, w io.Writer) (int, error) {
	timeout := flags.Duration("timeout", 30*time.Minute, "")
	pos, err := positional(flags, args)
	if err != nil || len(pos) != 1 {
		return 0, errUsage
	}
	id := pos[0]
	ctx, cancel := context.WithTimeout(context.Background(), *timeout)
	defer cancel()
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return 0, err
	}
	defer func() { c.close() }()
	a, ok := c.agents[id]
	if !ok {
		return 0, fmt.Errorf("no session %s", id)
	}
	if !a.Attached {
		return 0, fmt.Errorf("session %s doesn't report its status", id)
	}
	for {
		if _, _, over := outcome(c.agents[id]); over {
			c.settle(id)
			if code, word, over := outcome(c.agents[id]); over {
				fmt.Fprintln(w, word)
				return code, nil
			}
			// settle ends on a read whose deadline passed, which closes the conn.
			c.close()
			next, err := dialClient(context.Background(), config.Sock())
			if err != nil {
				return 0, err
			}
			c = next
			if _, ok := c.agents[id]; !ok {
				fmt.Fprintln(w, "closed")
				return exitClosed, nil
			}
			continue
		}
		if _, _, _, err := c.read(ctx); err != nil {
			if ctx.Err() != nil {
				fmt.Fprintln(w, "timeout")
				return exitTimeout, nil
			}
			return 0, err
		}
	}
}

// settle returns once nothing has come about id for settleFor. A turn's end
// comes by hook, its last lines by the transcript tail, which polls.
func (c *client) settle(id string) {
	deadline := time.Now().Add(settleFor)
	for {
		ctx, cancel := context.WithDeadline(c.ctx, deadline)
		_, _, raw, err := c.read(ctx)
		cancel()
		if err != nil {
			return
		}
		var m struct {
			AgentID string `json:"agentId"`
			Agent   struct {
				ID string `json:"id"`
			} `json:"agent"`
		}
		if json.Unmarshal(raw, &m) == nil && (m.AgentID == id || m.Agent.ID == id) {
			deadline = time.Now().Add(settleFor)
		}
	}
}

type newTicket struct {
	project, provider, base, title, prompt string
	start                                  bool
}

func parseTicket(flags *flag.FlagSet, args []string) (newTicket, error) {
	var n newTicket
	flags.StringVar(&n.project, "project", "", "")
	flags.StringVar(&n.provider, "provider", "claude", "")
	flags.StringVar(&n.base, "base", "", "")
	flags.StringVar(&n.title, "title", "", "")
	flags.BoolVar(&n.start, "start", false, "")
	pos, err := positional(flags, args)
	if err != nil || len(pos) != 1 {
		return n, errUsage
	}
	n.prompt = pos[0]
	return n, nil
}

// firstLine is what the Board's composer would title prompt.
func firstLine(prompt string) string {
	line, _, _ := strings.Cut(prompt, "\n")
	runes := []rune(strings.TrimSpace(line))
	return string(runes[:min(len(runes), proto.MaxTicketTitle)])
}

var errBadTicket = fmt.Errorf("a ticket needs a prompt, a known --provider, and a --title of at most %d characters", proto.MaxTicketTitle)

func ticketFor(list []proto.Project, cwd string, n newTicket) (proto.Ticket, error) {
	p, err := pickProject(list, n.project, cwd)
	if err != nil {
		return proto.Ticket{}, err
	}
	prompt := strings.TrimSpace(n.prompt)
	t := proto.Ticket{Title: cmp.Or(n.title, firstLine(prompt)), Prompt: prompt, Provider: n.provider, Folder: p.Path, Base: n.base}
	if !t.Valid() {
		return t, errBadTicket
	}
	return t, nil
}

func ticketCreateCmd(flags *flag.FlagSet, args []string, w io.Writer) error {
	n, err := parseTicket(flags, args)
	if err != nil {
		return err
	}
	if n.prompt, err = promptText(n.prompt); err != nil {
		return err
	}
	c, list, cwd, err := openProjects()
	if err != nil {
		return err
	}
	defer c.close()
	t, err := ticketFor(list, cwd, n)
	if err != nil {
		return err
	}
	id, err := c.saveTicket(t, n.start)
	if err != nil {
		return err
	}
	_, err = fmt.Fprintln(w, id)
	return err
}

func (c *client) saveTicket(t proto.Ticket, start bool) (string, error) {
	draft := map[string]any{"id": t.ID, "title": t.Title, "prompt": t.Prompt, "provider": t.Provider, "folder": t.Folder, "base": t.Base}
	raw, err := c.call("ack", map[string]any{"type": "board.save", "ticket": draft, "start": start})
	if err != nil {
		return "", err
	}
	var ack proto.Ack
	if err := json.Unmarshal(raw, &ack); err != nil {
		return "", err
	}
	return ack.TicketID, nil
}

func (c *client) tickets() ([]proto.Ticket, error) {
	if !c.owns {
		return nil, errInsideTerminal
	}
	for !c.boarded {
		if _, _, _, err := c.read(c.ctx); err != nil {
			return nil, err
		}
	}
	return c.board, nil
}

func (c *client) ticket(id string) (proto.Ticket, error) {
	list, err := c.tickets()
	if err != nil {
		return proto.Ticket{}, err
	}
	i := slices.IndexFunc(list, func(t proto.Ticket) bool { return t.ID == id })
	if i < 0 {
		return proto.Ticket{}, fmt.Errorf("no ticket %s", id)
	}
	return list[i], nil
}

// ticketState is what a live ticket's session is doing, or where any other ticket sits.
func ticketState(t proto.Ticket) string {
	if t.State == proto.TicketLive {
		return t.Status
	}
	return t.State
}

func ticketListCmd(flags *flag.FlagSet, args []string, w io.Writer) error {
	asJSON := flags.Bool("json", false, "")
	if pos, err := positional(flags, args); err != nil || len(pos) != 0 {
		return errUsage
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return err
	}
	defer c.close()
	list, err := c.tickets()
	if err != nil {
		return err
	}
	if *asJSON {
		return printJSON(w, list)
	}
	tw := tabwriter.NewWriter(w, 0, 4, 2, ' ', 0)
	for _, t := range list {
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\n", t.ID, ticketState(t), t.Provider, filepath.Base(t.Folder), t.Title)
	}
	return tw.Flush()
}

func ticketReadCmd(flags *flag.FlagSet, args []string, w io.Writer) error {
	asJSON := flags.Bool("json", false, "")
	pos, err := positional(flags, args)
	if err != nil || len(pos) != 1 {
		return errUsage
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return err
	}
	defer c.close()
	t, err := c.ticket(pos[0])
	if err != nil {
		return err
	}
	if *asJSON {
		return printJSON(w, t)
	}
	_, err = fmt.Fprintf(w, "%s\n%s %s %s\n\n%s\n", t.Title, ticketState(t), t.Provider, t.Folder, t.Prompt)
	return err
}

// parseTicketEdit returns the ticket's id and only the fields the command set,
// so an empty --base clears the base and a flag left out keeps its value.
func parseTicketEdit(flags *flag.FlagSet, args []string) (string, map[string]string, error) {
	for _, field := range []string{"title", "provider", "base", "prompt"} {
		flags.String(field, "", "")
	}
	pos, err := positional(flags, args)
	if err != nil || len(pos) != 1 || flags.NFlag() == 0 {
		return "", nil, errUsage
	}
	changes := map[string]string{}
	flags.Visit(func(f *flag.Flag) { changes[f.Name] = f.Value.String() })
	return pos[0], changes, nil
}

func editTicket(t proto.Ticket, changes map[string]string) (proto.Ticket, error) {
	for field, value := range map[string]*string{"title": &t.Title, "provider": &t.Provider, "base": &t.Base, "prompt": &t.Prompt} {
		if to, set := changes[field]; set {
			*value = to
		}
	}
	if !t.Valid() {
		return t, errBadTicket
	}
	return t, nil
}

func ticketUpdateCmd(flags *flag.FlagSet, args []string) error {
	id, changes, err := parseTicketEdit(flags, args)
	if err != nil {
		return err
	}
	if prompt, set := changes["prompt"]; set {
		if changes["prompt"], err = promptText(prompt); err != nil {
			return err
		}
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return err
	}
	defer c.close()
	t, err := c.ticket(id)
	if err != nil {
		return err
	}
	if t, err = editTicket(t, changes); err != nil {
		return err
	}
	_, err = c.saveTicket(t, false)
	return err
}

func ticketDeleteCmd(flags *flag.FlagSet, args []string) error {
	pos, err := positional(flags, args)
	if err != nil || len(pos) != 1 {
		return errUsage
	}
	c, err := dialClient(context.Background(), config.Sock())
	if err != nil {
		return err
	}
	defer c.close()
	_, err = c.call("ack", map[string]any{"type": "board.delete", "ticketId": pos[0]})
	return err
}

func skillInstall() error {
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	return cliskill.Install(home)
}
