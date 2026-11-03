# `anywhere` Command Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Scripts and agents running outside Anywhere on the user's Mac can list projects and sessions, start a session, prompt it, wait for it and read its reply, with an `anywhere` command and a skill that teaches agents to use it.

**Architecture:** `anywhere` is the pocketd binary, reached by a symlink and told apart by `argv[0]`. It speaks the existing WebSocket protocol over pocketd's unix socket, where a process outside every Terminal is already the owner, so it needs no API key, no port and no new daemon verb. The skill is embedded in the binary. A Settings button makes the symlink in `/usr/local/bin` behind macOS's admin prompt and installs the skill.

**Toolset** (paths relative to the repo root):

- First, in a worktree without it: `scripts/build-ghostty.sh`. `internal/vt` links `third_party/ghostty/zig-out`, and `go build` fails with `'ghostty/vt.h' file not found` until it exists.
- pocketd, CLI unit tests: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 ./cmd/pocketd`
- pocketd, one e2e test: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 -run '<regex>' ./e2e`
- pocketd, full, at every PR boundary: `cd packages/pocketd && env -u POCKETD_SOCK go vet ./... && env -u POCKETD_SOCK go test -race -count=1 ./...`
- desktop, one crate: `cd packages/desktop && cargo test -p daemon`, `cd packages/desktop && cargo test -p pocket -- <filter>`
- desktop, full, for PR 5: `cd packages/desktop && cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`. No new warnings.
- Screens, for PR 5: `.ui-review/fixture/capture.sh <dir> <name>=<steps>…`, before and after.

**Read first:**

- `CLAUDE.md`: where desktop code goes, test naming, the pre-commit commands.
- `CONTEXT.md`: Session, Agent, Attached, Status, Seen. A turn that ends while its session is seen goes to Idle, never Done.
- `docs/designs/2026-09-30-no-self-approval.md` §5 and line 299: why a process outside every Terminal is the owner, and the reparent residual this command makes easier to use.
- `packages/pocketd/internal/peer/classify.go`: `Classify`.
- `packages/pocketd/internal/peer/check.go`: `Principal.Check`, the refusal the CLI rewords.
- `packages/pocketd/internal/ops/ops.go:174-215`: the unix socket also serves the WebSocket; `route` sniffs the first byte.
- `packages/pocketd/e2e/harness_test.go:326-345`: `Harness.Owner`, the same connection this client makes.
- `packages/pocketd/cmd/pocketd/worktree.go`: the style of a CLI verb here.

**Settled decisions (don't re-open):**

- Callers are processes on this Mac, outside Anywhere's terminals. Agents inside them stay read-only.
- No `permission.resolve`, git, automation, or anything beyond the verbs listed in the skill.
- `session`, not `agent`, in the command's words: it's what the app shows.
- `--access` defaults to `settings` and takes every level, `full` included. The skill tells agents not to pick one unasked.
- `--provider` defaults to `claude`. `--project` defaults to the project holding the cwd. The default worktree is the project's main one, even when the cwd is in another of its worktrees.
- `--new-worktree` needs a name.
- Sessions run in their TUI (`Chat` false), so they look like any other.
- The CLI never sends `agent.view` or `agent.seen`, so a finished session still shows in Up next.
- `wait` exits at once on anything that needs handling: 0 done, 1 command error, 2 usage, 3 failed, 4 needs you, 5 timeout, 6 closed.
- pocketd not running is an error. The command doesn't start it.
- The button shows only in release builds on macOS. A dev bundle has no pocketd inside it (`scripts/bundle-dev.sh`).

**Dry run.** The code in PRs 1–4 was built and passed in a scratch copy of `packages/pocketd`, with the e2e tests of PRs 1–3 repeated 10 times. `cli.rs` in PR 5 passed under `rustc --test` with a stub `command`. The `pocket` crate changes in PR 5 have not been compiled.

---

## PR 1: Read-only commands

**Scope:** `anywhere project list`, `session list` and `session read`. Nothing installs the command yet, so it reaches nobody until PR 5.
**Depends on:** nothing.
**Done when:** the full pocketd suite is green and a symlink named `anywhere` lists the projects of a scratch pocketd.

### Task 1.1: e2e harness and failing tests

**Files:**
- Modify: `packages/pocketd/e2e/harness_test.go:45-48` (in `TestMain`)
- Create: `packages/pocketd/e2e/anywhere_test.go`

**Context:** e2e tests start a real pocketd against a throwaway home, with `fakeclaude` first on PATH. `launchReady("")` also makes a repo, `h.Repo`, registered as a project. `create`, `inRepo` and `reply` live in `e2e/launch_test.go`.

**Step 1: Add the symlink in `TestMain`**, just before `binDir = dir`:

```go
	if err := os.Symlink(filepath.Join(dir, "pocketd"), filepath.Join(dir, "anywhere")); err != nil {
		panic(err)
	}
```

**Step 2: Write the failing tests** in `e2e/anywhere_test.go`:

```go
//go:build unix

package e2e

import (
	"bytes"
	"encoding/json"
	"errors"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

// anywhere runs the `anywhere` command from h.Repo, as a process outside every Terminal.
func (h *Harness) anywhere(args ...string) (stdout, stderr string, code int) {
	h.t.Helper()
	cmd := exec.Command(filepath.Join(binDir, "anywhere"), args...)
	cmd.Env, cmd.Dir = h.Env, h.Repo
	var out, errOut bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &errOut
	if err := cmd.Run(); err != nil && !errors.As(err, new(*exec.ExitError)) {
		h.t.Fatal(err)
	}
	return strings.TrimSpace(out.String()), strings.TrimSpace(errOut.String()), cmd.ProcessState.ExitCode()
}

func TestAnywhereListsTheProjectsAsJSON(t *testing.T) {
	h := Start(t, launchReady(""))
	out, errOut, code := h.anywhere("project", "list", "--json")
	var list []proto.Project
	if err := json.Unmarshal([]byte(out), &list); code != 0 || err != nil || len(list) != 1 || list[0].Path != h.Repo {
		t.Fatalf("exited %d, %s, %s", code, out, errOut)
	}
}

func TestAnywhereReadsTheLastReply(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner()
	create(o, "r1", inRepo(h, map[string]any{"prompt": "hello"}))
	id := reply(o, "agent.created", "error").AgentID
	h.eventually("the reply", func() bool {
		out, _, code := h.anywhere("session", "read", id)
		return code == 0 && out == "echo: hello"
	})
}

func TestAnywhereWithoutArgumentsPrintsUsage(t *testing.T) {
	h := Start(t, launchReady(""))
	if _, errOut, code := h.anywhere(); code != 2 || !strings.HasPrefix(errOut, "usage: anywhere") {
		t.Fatalf("exited %d: %s", code, errOut)
	}
}
```

**Step 3: Run them to verify they fail**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 -run 'Anywhere' ./e2e`
Expected: FAIL. Without the dispatch in Task 1.2 the symlink is plain pocketd, so the usage test sees `usage: pocketd`.

### Task 1.2: The client

**Files:**
- Create: `packages/pocketd/cmd/pocketd/anywhere_client.go`

**Context:** hello must be the first message. After `hello.ok`, pocketd sends `agent.list` with no id, then pushes `agent.update` for every change. Replies carry the request's `id`. Other messages come in between, and some have a field named `agent` that isn't an `AgentSummary`, so the client decodes the envelope first and each body only by type. `coder/websocket` closes the conn when a `Read`'s context ends, which ends the command everywhere except `wait`, which dials again (Task 3.2).

**Step 1: Write the client**

```go
package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"strconv"

	"github.com/coder/websocket"

	"pocketd/internal/proto"
)

// client is one conn to pocketd's WebSocket protocol over its unix socket,
// where a process outside every Terminal is the owner.
type client struct {
	ctx    context.Context
	ws     *websocket.Conn
	calls  int
	listed bool
	agents map[string]proto.AgentSummary
}

func dialClient(ctx context.Context, sock string) (*client, error) {
	unix := &http.Client{Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", sock)
	}}}
	ws, _, err := websocket.Dial(ctx, "ws://localhost/", &websocket.DialOptions{HTTPClient: unix})
	if err != nil {
		return nil, fmt.Errorf("can't reach Anywhere's service at %s: %w", sock, err)
	}
	// agent.list and a timeline page can be far over the 32KB default.
	ws.SetReadLimit(-1)
	c := &client{ctx: ctx, ws: ws, agents: map[string]proto.AgentSummary{}}
	if _, err := c.call("hello.ok", map[string]any{"type": "hello", "clientId": "anywhere", "protocolVersion": proto.MaxVersion}); err != nil {
		ws.CloseNow()
		return nil, err
	}
	for !c.listed {
		if _, _, _, err := c.read(ctx); err != nil {
			ws.CloseNow()
			return nil, err
		}
	}
	return c, nil
}

func (c *client) close() { c.ws.CloseNow() }

// read returns pocketd's next message, keeping the agents up to date.
func (c *client) read(ctx context.Context) (kind, id string, raw []byte, err error) {
	if _, raw, err = c.ws.Read(ctx); err != nil {
		return "", "", nil, err
	}
	var head struct {
		Type string `json:"type"`
		ID   string `json:"id"`
	}
	if err := json.Unmarshal(raw, &head); err != nil {
		return "", "", nil, err
	}
	switch head.Type {
	case "agent.list":
		var l proto.AgentList
		if err := json.Unmarshal(raw, &l); err != nil {
			return "", "", nil, err
		}
		for _, a := range l.Agents {
			c.saw(a)
		}
		c.listed = true
	case "agent.update":
		var u proto.AgentUpdate
		if err := json.Unmarshal(raw, &u); err != nil {
			return "", "", nil, err
		}
		c.saw(u.Agent)
	}
	return head.Type, head.ID, raw, nil
}

func (c *client) saw(a proto.AgentSummary) {
	c.agents[a.ID] = a
}

// call sends m under a fresh id and returns the reply of type want. A verb may
// send others under that id first, as agent.create does.
func (c *client) call(want string, m map[string]any) ([]byte, error) {
	c.calls++
	id := strconv.Itoa(c.calls)
	m["id"] = id
	raw, err := json.Marshal(m)
	if err != nil {
		return nil, err
	}
	if err := c.ws.Write(c.ctx, websocket.MessageText, raw); err != nil {
		return nil, err
	}
	for {
		kind, got, raw, err := c.read(c.ctx)
		if err != nil {
			return nil, err
		}
		switch {
		case got != id:
		case kind == want:
			return raw, nil
		case kind == "error":
			var e proto.Error
			if err := json.Unmarshal(raw, &e); err != nil {
				return nil, err
			}
			return nil, refusal(e)
		}
	}
}

func refusal(e proto.Error) error {
	if e.Code == proto.CodeScopeDenied {
		return errors.New("a process inside an Anywhere terminal can only read sessions; run this from outside Anywhere")
	}
	return errors.New(e.Message)
}
```

**Step 2: Build it**

Run: `cd packages/pocketd && go build ./cmd/pocketd && rm pocketd`
Expected: no output. Go doesn't report unused functions, so this builds before Task 1.3 calls it.

### Task 1.3: Dispatch and the read commands

**Files:**
- Create: `packages/pocketd/cmd/pocketd/anywhere.go`
- Create: `packages/pocketd/cmd/pocketd/anywhere_test.go`
- Modify: `packages/pocketd/cmd/pocketd/main.go:3-14` (imports), `:18` (start of `main`)

**Context:** `worktree.go` is the pattern: a `flag.FlagSet`, a text form and a `--json` form. Go's `flag` stops at the first positional argument, and agents will write `wait <id> --timeout 10m`, so `positional` keeps parsing past it.

**Step 1: Write the failing unit tests** in `cmd/pocketd/anywhere_test.go`:

```go
package main

import (
	"errors"
	"flag"
	"io"
	"reflect"
	"testing"
)

func TestPositionalTakesFlagsOnEitherSideOfTheArguments(t *testing.T) {
	flags := flag.NewFlagSet("t", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	timeout := flags.String("timeout", "", "")
	pos, err := positional(flags, []string{"abc", "--timeout", "10m"})
	if err != nil || *timeout != "10m" || !reflect.DeepEqual(pos, []string{"abc"}) {
		t.Fatalf("pos = %v, timeout = %s, err = %v", pos, *timeout, err)
	}
}

func TestLastReplyIsWhatCameAfterTheLastUserMessage(t *testing.T) {
	items := []timelineItem{{"user", "one"}, {"assistant", "old"}, {"user", "two"}, {"tool", ""}, {"assistant", "first"}, {"assistant", "second"}}
	if got := lastReply(items); got != "first\n\nsecond" {
		t.Fatalf("got %q", got)
	}
}

func TestAnUnknownVerbIsAUsageError(t *testing.T) {
	if _, err := anywhere([]string{"session", "frob"}, io.Discard); !errors.Is(err, errUsage) {
		t.Fatalf("err = %v", err)
	}
}
```

**Step 2: Run them to verify they fail**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 ./cmd/pocketd`
Expected: FAIL to compile with `undefined: positional`.

**Step 3: Write the commands** in `cmd/pocketd/anywhere.go`:

```go
package main

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"slices"
	"strings"
	"text/tabwriter"

	"pocketd/internal/config"
	"pocketd/internal/proto"
)

const anywhereUsage = `usage: anywhere project list [--json]
       anywhere session list [--json]
       anywhere session read [--limit 200] [--json] <id>`

const (
	exitError = 1
	exitUsage = 2
)

var errUsage = errors.New("usage")

func anywhereMain(args []string, stdout, stderr io.Writer) int {
	code, err := anywhere(args, stdout)
	switch {
	case errors.Is(err, errUsage):
		fmt.Fprintln(stderr, anywhereUsage)
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
	return l.Projects, json.Unmarshal(raw, &l)
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
```

**Step 4: Dispatch on the binary's name.** In `main.go`, add `"path/filepath"` to the imports and make this the first statement of `main`, before the `len(os.Args) < 2` check:

```go
	if filepath.Base(os.Args[0]) == "anywhere" {
		os.Exit(anywhereMain(os.Args[1:], os.Stdout, os.Stderr))
	}
```

**Step 5: Run the tests to verify they pass**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 ./cmd/pocketd && env -u POCKETD_SOCK go test -count=1 -run 'Anywhere' ./e2e`
Expected: PASS.

---

## PR 2: Drive sessions

**Scope:** `anywhere session create`, `prompt` and `close`. Still not installed.
**Depends on:** PR 1.
**Done when:** the full pocketd suite is green, and a process inside a Terminal gets a plain refusal.

### Task 2.1: Build a launch spec from flags

**Files:**
- Modify: `packages/pocketd/cmd/pocketd/anywhere.go`
- Modify: `packages/pocketd/cmd/pocketd/anywhere_test.go`

**Context:** `proto.LaunchSpec` needs a project path and exactly one of `Checkout.Worktree` (a path) and `Checkout.New`. The picking is pure so it can be tested without a daemon.

**Step 1: Write the failing tests.** Add to `anywhere_test.go`, with `"strings"` and `"pocketd/internal/proto"` in its imports:

```go
var twoProjects = []proto.Project{
	{Path: "/code/api", Name: "api", Worktrees: []proto.Worktree{{Name: "api", Path: "/code/api", IsMain: true}, {Name: "calm-otter", Path: "/wt/calm-otter"}}},
	{Path: "/code/web", Name: "web", Worktrees: []proto.Worktree{{Name: "web", Path: "/code/web", IsMain: true}}},
}

func TestLaunchSpecFindsTheProjectHoldingTheFolderAndItsMainWorktree(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/wt/calm-otter/src", newSession{provider: "claude", access: "settings"})
	if err != nil || spec.Project != "/code/api" || spec.Checkout.Worktree != "/code/api" {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func TestLaunchSpecTakesAProjectByNameAndAWorktreeByName(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/elsewhere", newSession{project: "api", worktree: "calm-otter"})
	if err != nil || spec.Checkout.Worktree != "/wt/calm-otter" {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func TestLaunchSpecAsksForANewWorktreeByName(t *testing.T) {
	spec, err := launchSpec(twoProjects, "/code/web", newSession{newWorktree: "calm-otter"})
	if err != nil || spec.Project != "/code/web" || spec.Checkout.New == nil || spec.Checkout.New.Name != "calm-otter" || spec.Checkout.Worktree != "" {
		t.Fatalf("spec = %+v, err = %v", spec, err)
	}
}

func TestLaunchSpecRefusesAFolderOutsideEveryProject(t *testing.T) {
	if _, err := launchSpec(twoProjects, "/code/apiary", newSession{}); err == nil || !strings.Contains(err.Error(), "--project") {
		t.Fatalf("err = %v", err)
	}
}

func TestLaunchSpecRefusesANameTwoProjectsShare(t *testing.T) {
	list := []proto.Project{{Path: "/a/x", Name: "x"}, {Path: "/b/x", Name: "x"}}
	if _, err := launchSpec(list, "/", newSession{project: "x"}); err == nil || !strings.Contains(err.Error(), "path") {
		t.Fatalf("err = %v", err)
	}
}
```

**Step 2: Run them to verify they fail**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 ./cmd/pocketd`
Expected: FAIL to compile with `undefined: launchSpec`.

**Step 3: Write the implementation.** Add to `anywhere.go`, with `"path/filepath"` in its imports:

```go
type newSession struct{ project, worktree, newWorktree, provider, access, prompt string }

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
	spec := proto.LaunchSpec{Project: p.Path, Provider: n.provider, Access: n.access, Prompt: n.prompt}
	if n.newWorktree != "" {
		spec.Checkout.New = &proto.NewWorktree{Name: n.newWorktree}
		return spec, nil
	}
	w, err := pickWorktree(p, n.worktree)
	if err != nil {
		return proto.LaunchSpec{}, err
	}
	spec.Checkout.Worktree = w.Path
	return spec, nil
}
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 ./cmd/pocketd`
Expected: PASS.

### Task 2.2: create, prompt and close

**Files:**
- Modify: `packages/pocketd/e2e/anywhere_test.go`
- Modify: `packages/pocketd/cmd/pocketd/anywhere.go`
- Modify: `packages/pocketd/cmd/pocketd/anywhere_client.go`

**Context:** Right after a prompt the agent is still idle, so a `wait` run then would report the turn over before it began. `create --prompt` and `prompt` therefore return only once they've seen the agent working or needing the user. Every status change is published, and hello subscribes before any verb runs, so the client can't miss even a turn that's over in milliseconds. An agent that isn't Attached reports no status, so `prompt` doesn't wait for one.

Don't filter `began` by anything but the status: a fast turn may go working → done before `agent.created` reaches the client, and `create` learns the id only then. That's why `saw` records it for every agent.

**Step 1: Write the failing tests.** Add to `e2e/anywhere_test.go`:

```go
// session creates a session on the fake claude with prompt, which may be empty, and returns its id.
func (h *Harness) session(prompt string) string {
	h.t.Helper()
	args := []string{"session", "create", "--access", "ask"}
	if prompt != "" {
		args = append(args, "--prompt", prompt)
	}
	out, errOut, code := h.anywhere(args...)
	if code != 0 || out == "" {
		h.t.Fatalf("create exited %d: %s %s", code, out, errOut)
	}
	return out
}

func TestAnywhereListsASessionItCreated(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hello")
	out, _, code := h.anywhere("session", "list")
	if code != 0 || !strings.Contains(out, id) {
		t.Fatalf("exited %d: %s", code, out)
	}
}

func TestAnywhereSendsASessionAPrompt(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("")
	if out, errOut, code := h.anywhere("session", "prompt", id, "again"); code != 0 {
		t.Fatalf("prompt exited %d: %s %s", code, out, errOut)
	}
	h.eventually("the reply", func() bool {
		out, _, _ := h.anywhere("session", "read", id)
		return out == "echo: again"
	})
}

func TestAnywhereClosesASession(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("")
	if out, errOut, code := h.anywhere("session", "close", id); code != 0 {
		t.Fatalf("close exited %d: %s %s", code, out, errOut)
	}
	h.eventually("the session to go", func() bool {
		out, _, _ := h.anywhere("session", "list")
		return !strings.Contains(out, id)
	})
}

func TestAnywhereOpensANewWorktreeWhenAsked(t *testing.T) {
	h := Start(t, launchReady(""))
	out, errOut, code := h.anywhere("session", "create", "--access", "ask", "--new-worktree", "calm-otter", "--prompt", "hello")
	if code != 0 {
		t.Fatalf("exited %d: %s %s", code, out, errOut)
	}
	list, _, _ := h.anywhere("session", "list", "--json")
	var agents []proto.AgentSummary
	if err := json.Unmarshal([]byte(list), &agents); err != nil || len(agents) != 1 || !strings.HasSuffix(agents[0].Cwd, "/calm-otter") {
		t.Fatalf("got %s", list)
	}
}

func TestAProcessInsideATerminalIsToldItCanOnlyRead(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.Spawn("/bin/sh", "-c", filepath.Join(binDir, "anywhere")+" session close nope; sleep 30")
	h.WaitScreen(id, "can only read")
}
```

**Step 2: Run them to verify they fail**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 -run 'Anywhere|InsideATerminalIsTold' ./e2e`
Expected: FAIL. `create exited 2`, since the verb doesn't exist yet.

**Step 3: Track who has been in a turn.** In `anywhere_client.go`, add a field to `client`:

```go
	// began holds each agent seen in a turn. hello subscribes before any verb, so no turn slips past.
	began map[string]bool
```

Initialize it in `dialClient`:

```go
	c := &client{ctx: ctx, ws: ws, agents: map[string]proto.AgentSummary{}, began: map[string]bool{}}
```

Replace `saw`:

```go
func (c *client) saw(a proto.AgentSummary) {
	c.agents[a.ID] = a
	if a.Status == "working" || a.Status == "needsYou" {
		c.began[a.ID] = true
	}
}
```

**Step 4: Write the commands.** In `anywhere.go`:

Add `"os"` and `"time"` to the imports. Add to `anywhereUsage`:

```
       anywhere session create [--project <name|path>] [--worktree <name> | --new-worktree <name>] [--provider claude|codex] [--access settings|ask|edits|auto|full] [--prompt <text>]
       anywhere session prompt <id> <text|->
       anywhere session close <id>
```

Add these cases to the switch in `anywhere`:

```go
	case "session create":
		return 0, sessionCreateCmd(flags, rest, stdout)
	case "session prompt":
		return 0, sessionPromptCmd(flags, rest)
	case "session close":
		return 0, sessionCloseCmd(flags, rest)
```

Add below the `exit*` constants:

```go
// turnStart is how long a prompted agent has to start its turn.
const turnStart = 30 * time.Second
```

Add the commands:

```go
func sessionCreateCmd(flags *flag.FlagSet, args []string, w io.Writer) error {
	var n newSession
	flags.StringVar(&n.project, "project", "", "")
	flags.StringVar(&n.worktree, "worktree", "", "")
	flags.StringVar(&n.newWorktree, "new-worktree", "", "")
	flags.StringVar(&n.provider, "provider", "claude", "")
	flags.StringVar(&n.access, "access", "settings", "")
	flags.StringVar(&n.prompt, "prompt", "", "")
	if pos, err := positional(flags, args); err != nil || len(pos) != 0 || n.worktree != "" && n.newWorktree != "" {
		return errUsage
	}
	cwd, err := os.Getwd()
	if err != nil {
		return err
	}
	if cwd, err = filepath.EvalSymlinks(cwd); err != nil {
		return err
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
	if n.prompt != "" {
		if err := c.awaitTurn(created.AgentID); err != nil {
			return err
		}
	}
	_, err = fmt.Fprintln(w, created.AgentID)
	return err
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

func sessionPromptCmd(flags *flag.FlagSet, args []string) error {
	pos, err := positional(flags, args)
	if err != nil || len(pos) != 2 {
		return errUsage
	}
	id, text := pos[0], pos[1]
	if text == "-" {
		in, err := io.ReadAll(os.Stdin)
		if err != nil {
			return err
		}
		text = string(in)
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
```

**Step 5: Run the tests to verify they pass**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 -run 'Anywhere|InsideATerminalIsTold' ./e2e`
Expected: PASS.

---

## PR 3: `wait`

**Scope:** `anywhere session wait` and its exit codes.
**Depends on:** PR 2.
**Done when:** the full pocketd suite is green, and the wait-then-read tests pass 10 times in a row.

### Task 3.1: A turn that doesn't end, for tests

**Files:**
- Modify: `packages/pocketd/e2e/fakeclaude/main.go:9-10` (doc comment), `:190-196` (the switch in `main`)

**Context:** The timeout and close tests need an agent stuck in a turn. `select {}` won't do: with nothing else running, Go kills it as a deadlock and the agent exits 2.

**Step 1: Add the line to the doc comment**, after `desk <cmd>`:

```go
//	hang        stay in the turn until killed
```

**Step 2: Add the case** before `default:` in the switch on `line`:

```go
		case line == "hang":
			time.Sleep(time.Hour)
```

**Step 3: Build it**

Run: `cd packages/pocketd && go build -o /dev/null ./e2e/fakeclaude`
Expected: no output.

### Task 3.2: wait

**Files:**
- Modify: `packages/pocketd/e2e/anywhere_test.go`
- Modify: `packages/pocketd/cmd/pocketd/anywhere_test.go`
- Modify: `packages/pocketd/cmd/pocketd/anywhere.go`

**Context:**

- A turn that ends while the session is on screen goes to `idle`, never `done`. So `outcome` treats both as a finished turn. Don't test for `done` alone: `wait` would hang exactly when the user is looking.
- **The dry run caught this one.** The hook reports the turn's end at once, but the reply reaches the timeline through `claude.Tail`, which polls every 100ms (`internal/claude/transcript.go:762`). Without `settle`, `read` right after `wait` returned nothing or the previous reply in 3 of 8 runs. Don't remove it.
- `settle` counts only messages about this session, so another busy session can't hold `wait` open.
- If the status moved on during `settle` (a queued prompt started a turn), `wait` carries on. `settle` ends on a read whose deadline passed, which closes the conn, so `wait` dials again first.
- pocketd restarting mid-wait drops the conn, which exits 1. The skill tells agents to run `wait` again.

**Step 1: Write the failing tests.** Add to `cmd/pocketd/anywhere_test.go`:

```go
func TestOutcomeTellsEachWayATurnStops(t *testing.T) {
	for _, c := range []struct {
		a    proto.AgentSummary
		code int
		word string
		over bool
	}{
		{proto.AgentSummary{Status: "working"}, 0, "", false},
		{proto.AgentSummary{Status: "needsYou"}, exitNeedsYou, "needsYou", true},
		{proto.AgentSummary{Status: "closed"}, exitClosed, "closed", true},
		{proto.AgentSummary{Status: "done", Failed: true}, exitFailed, "failed", true},
		{proto.AgentSummary{Status: "done"}, 0, "done", true},
		{proto.AgentSummary{Status: "idle"}, 0, "done", true},
	} {
		if code, word, over := outcome(c.a); code != c.code || word != c.word || over != c.over {
			t.Errorf("%s: got %d %q %v", c.a.Status, code, word, over)
		}
	}
}
```

Add to `e2e/anywhere_test.go`, with `"time"` in its imports:

```go
func TestAnywhereCreatesASessionThenWaitsForItAndReadsTheReply(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hello")
	if out, errOut, code := h.anywhere("session", "wait", id); code != 0 || out != "done" {
		t.Fatalf("wait exited %d: %s %s", code, out, errOut)
	}
	if out, errOut, code := h.anywhere("session", "read", id); code != 0 || out != "echo: hello" {
		t.Fatalf("read exited %d: %q %s", code, out, errOut)
	}
}

func TestAWaitRightAfterAPromptWaitsForThatTurn(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hello")
	h.anywhere("session", "wait", id)
	if out, errOut, code := h.anywhere("session", "prompt", id, "again"); code != 0 {
		t.Fatalf("prompt exited %d: %s %s", code, out, errOut)
	}
	if out, _, code := h.anywhere("session", "wait", id); code != 0 || out != "done" {
		t.Fatalf("wait exited %d: %s", code, out)
	}
	if out, _, _ := h.anywhere("session", "read", id); out != "echo: again" {
		t.Fatalf("read = %q", out)
	}
}

func TestWaitStopsWithItsOwnCodeWhenTheAgentAsks(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("run ls")
	if out, errOut, code := h.anywhere("session", "wait", id); code != 4 || out != "needsYou" {
		t.Fatalf("wait exited %d: %s %s", code, out, errOut)
	}
}

func TestWaitTimesOutOnATurnThatDoesntEnd(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hang")
	if out, errOut, code := h.anywhere("session", "wait", "--timeout", "200ms", id); code != 5 || out != "timeout" {
		t.Fatalf("wait exited %d: %s %s", code, out, errOut)
	}
}

func TestWaitSaysSoWhenTheSessionCloses(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hang")
	done := make(chan string)
	go func() {
		out, _, code := h.anywhere("session", "wait", id)
		done <- out + string(rune('0'+code))
	}()
	time.Sleep(300 * time.Millisecond)
	if out, errOut, code := h.anywhere("session", "close", id); code != 0 {
		t.Fatalf("close exited %d: %s %s", code, out, errOut)
	}
	select {
	case got := <-done:
		if got != "closed6" {
			t.Fatalf("wait = %s", got)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("wait did not end")
	}
}
```

**Step 2: Run them to verify they fail**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 ./cmd/pocketd`
Expected: FAIL to compile with `undefined: outcome`.

**Step 3: Write the implementation.** In `anywhere.go`:

Add to `anywhereUsage`:

```
       anywhere session wait [--timeout 30m] <id>
```

Extend the `exit*` constants:

```go
const (
	exitError    = 1
	exitUsage    = 2
	exitFailed   = 3
	exitNeedsYou = 4
	exitTimeout  = 5
	exitClosed   = 6
)
```

Add below `turnStart`:

```go
// settleFor outlasts a few of claude.Tail's 100ms polls.
const settleFor = 500 * time.Millisecond
```

Add a case to the switch in `anywhere`:

```go
	case "session wait":
		return sessionWaitCmd(flags, rest, stdout)
```

Add the code:

```go
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
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 ./cmd/pocketd && env -u POCKETD_SOCK go test -count=1 -run 'Anywhere|AWaitRight|WaitStops|WaitTimes|WaitSays|InsideATerminalIsTold' ./e2e`
Expected: PASS.

**Step 5: Prove the race stays closed**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=10 -run 'AnywhereCreates|AWaitRight' ./e2e`
Expected: PASS.

---

## PR 4: The skill

**Scope:** `pocketd skill install`, and pocketd refreshing installed copies when it starts. Nothing calls install yet.
**Depends on:** PR 3, whose commands the skill describes.
**Done when:** the full pocketd suite is green.

### Task 4.1: Embed and install the skill

**Files:**
- Create: `packages/pocketd/internal/cliskill/SKILL.md`
- Create: `packages/pocketd/internal/cliskill/cliskill.go`
- Create: `packages/pocketd/internal/cliskill/cliskill_test.go`

**Context:** Claude reads `~/.claude/skills/<name>/SKILL.md`. Codex reads `~/.agents/skills/<name>/SKILL.md` (`internal/skills/skills.go:37-43`). Agents in an Anywhere account use another config folder and won't see the Claude copy, which is right: they can only read. Those that do see it are told what the refusal means.

The existing `internal/skills` package reads skills from disk for the slash menu. This one is unrelated, so it stays separate.

**Step 1: Write `SKILL.md`**

````markdown
---
name: anywhere
description: Run and follow Claude Code and Codex sessions in Anywhere, the user's desktop app for coding agents, with the `anywhere` command. Use when the user asks to start a session in Anywhere, hand work to another agent there, send a session a prompt, check on sessions, or read what one replied.
---

# anywhere

`anywhere` acts as the user on this Mac. Every session it starts shows in the Anywhere app like one the user opened.

## Commands

```sh
anywhere project list [--json]
anywhere session list [--json]
anywhere session create [--project <name|path>] [--worktree <name> | --new-worktree <name>] [--provider claude|codex] [--access settings|ask|edits|auto|full] [--prompt <text>]
anywhere session prompt <id> <text|->
anywhere session wait [--timeout 30m] <id>
anywhere session read [--limit 200] [--json] <id>
anywhere session close <id>
```

`create` prints the session's id. Without `--project` it takes the project holding the current folder. Without a worktree flag it runs in the project's main worktree, the same folder the user works in. `prompt <id> -` reads the text from stdin.

## Hand over a task

```sh
id=$(anywhere session create --new-worktree fix-login --prompt "Fix the login redirect")
anywhere session wait "$id"
anywhere session read "$id"
```

`create --prompt` and `prompt` return once the turn has started, so `wait` right after them waits for that turn.

`wait` prints one word and exits with its code:

| Exit | Word | What to do |
|---|---|---|
| 0 | `done` | `read` the reply |
| 1 | | The command failed; stderr says why. If the connection dropped, run `wait` again |
| 2 | | Fix the command line |
| 3 | `failed` | `read` the reply, tell the user |
| 4 | `needsYou` | The agent is waiting on the user. Tell them, then run `wait` again |
| 5 | `timeout` | The agent is still working. Run `wait` again or tell the user |
| 6 | `closed` | The session is gone |

## Rules

- Use `--new-worktree` when sessions run side by side, or when the user may be working in the main worktree.
- Leave `--access` out unless the user asks for a level. Pass `full` only when they ask for it by name.
- Permission prompts are the user's. Never try to answer one.
- Don't close a session you didn't create unless the user asks.
- If `anywhere` says a process inside an Anywhere terminal can only read sessions, you are running inside Anywhere. Don't retry; tell the user.
````

**Step 2: Write the failing tests** in `cliskill_test.go`:

```go
package cliskill

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

func TestInstallWritesTheSkillForClaudeAndCodex(t *testing.T) {
	home := t.TempDir()
	if err := Install(home); err != nil {
		t.Fatal(err)
	}
	for _, dir := range []string{".claude", ".agents"} {
		got, err := os.ReadFile(filepath.Join(home, dir, "skills", "anywhere", "SKILL.md"))
		if err != nil || !bytes.Equal(got, skill) {
			t.Fatalf("%s: %v", dir, err)
		}
	}
}

func TestRefreshOverwritesAnInstalledCopy(t *testing.T) {
	home := t.TempDir()
	if err := Install(home); err != nil {
		t.Fatal(err)
	}
	p := paths(home)[0]
	os.WriteFile(p, []byte("old"), 0o644)
	if err := Refresh(home); err != nil {
		t.Fatal(err)
	}
	if got, _ := os.ReadFile(p); !bytes.Equal(got, skill) {
		t.Fatalf("got %s", got)
	}
}

func TestRefreshAddsNothingToAHomeThatNeverInstalled(t *testing.T) {
	home := t.TempDir()
	if err := Refresh(home); err != nil {
		t.Fatal(err)
	}
	if entries, _ := os.ReadDir(home); len(entries) != 0 {
		t.Fatalf("got %v", entries)
	}
}
```

**Step 3: Run them to verify they fail**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 ./internal/cliskill`
Expected: FAIL to compile with `undefined: Install`.

**Step 4: Write `cliskill.go`**

```go
// Package cliskill puts the skill that teaches agents the `anywhere` command
// where Claude and Codex read skills.
package cliskill

import (
	_ "embed"
	"errors"
	"io/fs"
	"os"
	"path/filepath"

	"pocketd/internal/atomicfile"
)

//go:embed SKILL.md
var skill []byte

func paths(home string) []string {
	return []string{
		filepath.Join(home, ".claude", "skills", "anywhere", "SKILL.md"),
		filepath.Join(home, ".agents", "skills", "anywhere", "SKILL.md"),
	}
}

// Install writes the skill for Claude and Codex under the user's home.
func Install(home string) error {
	for _, p := range paths(home) {
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			return err
		}
		if err := atomicfile.Write(p, skill, 0o644); err != nil {
			return err
		}
	}
	return nil
}

// Refresh brings the copies Install wrote up to this binary's, and adds none.
func Refresh(home string) error {
	for _, p := range paths(home) {
		_, err := os.Stat(p)
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err == nil {
			err = atomicfile.Write(p, skill, 0o644)
		}
		if err != nil {
			return err
		}
	}
	return nil
}
```

**Step 5: Run the tests to verify they pass**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 ./internal/cliskill`
Expected: PASS.

### Task 4.2: Install on demand, refresh on start

**Files:**
- Modify: `packages/pocketd/e2e/anywhere_test.go`
- Modify: `packages/pocketd/cmd/pocketd/main.go` (the `usage` constant, and the switch in `main`)
- Modify: `packages/pocketd/cmd/pocketd/anywhere.go`
- Modify: `packages/pocketd/cmd/pocketd/serve.go:92` (after the `start` event), and its imports

**Context:** `serve` also runs after an in-place upgrade (ADR 0005), so an app update refreshes the skill without a new install. In `serve`, `home` is pocketd's own folder, so the user's home takes another name.

**Step 1: Write the failing test.** Add to `e2e/anywhere_test.go`, with `"os"` in its imports:

```go
func TestPocketdBringsAnInstalledSkillUpToDateWhenItStarts(t *testing.T) {
	h := Start(t)
	if out, code := h.Pocketd("skill", "install"); code != 0 {
		t.Fatalf("install exited %d: %s", code, out)
	}
	skill := filepath.Join(h.Home, ".claude", "skills", "anywhere", "SKILL.md")
	if err := os.WriteFile(skill, []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	h.Restart()
	if got, _ := os.ReadFile(skill); !strings.Contains(string(got), "name: anywhere") {
		t.Fatalf("got %s", got)
	}
}
```

**Step 2: Run it to verify it fails**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 -run 'BringsAnInstalled' ./e2e`
Expected: FAIL with `install exited 2`.

**Step 3: Add `skill install`.** In `main.go`, add ` | skill install` to the end of the `usage` constant, and add this case before `case os.Args[1] == "stats":`

```go
	case slices.Equal(os.Args[1:], []string{"skill", "install"}):
		err = skillInstall()
```

In `anywhere.go`, add `"pocketd/internal/cliskill"` to the imports and:

```go
func skillInstall() error {
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	return cliskill.Install(home)
}
```

**Step 4: Refresh on start.** In `serve.go`, add `"pocketd/internal/cliskill"` to the imports and, right after the `evs.Emit(events.Event{Kind: "start", …})` line:

```go
	if user, err := os.UserHomeDir(); err == nil {
		if err := cliskill.Refresh(user); err != nil {
			log.Printf("skill: %v", err)
		}
	}
```

**Step 5: Run the test to verify it passes**

Run: `cd packages/pocketd && env -u POCKETD_SOCK go test -count=1 -run 'BringsAnInstalled' ./e2e`
Expected: PASS.

---

## PR 5: The Settings button

**Scope:** Settings → General → Command line tool, and the README. This is what puts the command in users' hands.
**Depends on:** PR 4.
**Done when:** the desktop's full checks pass, the screens match, and the manual verification below is done on a release build.

### Task 5.1: Make the link and install the skill

**Files:**
- Create: `packages/desktop/crates/daemon/src/cli.rs`
- Modify: `packages/desktop/crates/daemon/src/daemon.rs:15`

**Context:** `/usr/local/bin` needs root, and `/etc/paths` has it, so every login shell finds the command. `osascript … with administrator privileges` shows macOS's own prompt and needs no helper. The pocketd lives in the bundle's helper app (`scripts/release-mac.sh:69-71`). A Sparkle update swaps the bundle in place, so the link survives it; moving the app breaks it, and the row then offers Install again.

**Step 1: Write `cli.rs` with its tests**

```rust
//! The `anywhere` command on the user's PATH: a link to the pocketd in the app, made once behind macOS's admin prompt.

use std::path::{Path, PathBuf};
use std::process::Command;

const LINK: &str = "/usr/local/bin/anywhere";

/// The pocketd in the bundle whose binary is `exe`, `Anywhere.app/Contents/MacOS/pocket-desktop`.
fn bundled_pocketd(exe: &Path) -> Option<PathBuf> {
    Some(exe.parent()?.parent()?.join("Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd"))
}

fn pocketd() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    bundled_pocketd(&exe).filter(|p| p.is_file()).ok_or_else(|| "This build has no pocketd in its bundle".to_string())
}

/// Whether the link points at this app's pocketd. Reads the disk; keep it off the UI thread.
pub fn installed() -> bool {
    pocketd().is_ok_and(|p| std::fs::read_link(LINK).is_ok_and(|target| target == p))
}

/// AppleScript that makes the link as root.
fn link_script(pocketd: &Path) -> String {
    let path = pocketd.display().to_string().replace('\\', "\\\\").replace('"', "\\\"");
    format!("do shell script \"mkdir -p /usr/local/bin && ln -sfn \" & quoted form of \"{path}\" & \" {LINK}\" with administrator privileges")
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// Makes the link and installs the skill. Blocks on the password prompt; keep it off the UI thread.
pub fn install() -> Result<(), String> {
    let pocketd = pocketd()?;
    run(crate::command("osascript").args(["-e", &link_script(&pocketd)]))?;
    run(crate::command(&pocketd).args(["skill", "install"]))
}

#[cfg(test)]
mod tests {
    use super::{bundled_pocketd, link_script};
    use std::path::{Path, PathBuf};

    #[test]
    fn the_bundled_pocketd_sits_in_the_helper_app() {
        let got = bundled_pocketd(Path::new("/Applications/Anywhere.app/Contents/MacOS/pocket-desktop"));
        assert_eq!(got, Some(PathBuf::from("/Applications/Anywhere.app/Contents/Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd")));
    }

    #[test]
    fn the_link_script_keeps_a_quote_in_the_path_inside_its_string() {
        assert!(link_script(Path::new("/Apps/A\"B.app/pocketd")).contains("quoted form of \"/Apps/A\\\"B.app/pocketd\""));
    }
}
```

**Step 2: Declare the module.** In `daemon.rs`, next to `pub mod service;`:

```rust
pub mod cli;
```

**Step 3: Run the tests**

Run: `cd packages/desktop && cargo test -p daemon cli`
Expected: PASS, 2 tests.

### Task 5.2: The Settings row

**Files:**
- Modify: `packages/desktop/crates/pocket/src/settings/host.rs:13-18` (the `Host` struct), `:46` (`Host::new`), `:146-160` (`load_host`)
- Modify: `packages/desktop/crates/pocket/src/settings/general.rs`
- Modify: `README.md`

**Context:** `render` must not touch the disk, so whether the command is installed is read in `load_host`, off the UI thread, when Settings opens, and kept on `Host`. `load_host` returns early while capturing, so screenshots show the row as not installed. `update_status` and its tests are the pattern for the row's words.

**Step 1: Write the failing test.** In `general.rs`'s `mod tests`, extend the `use super::{ROWS, update_status};` line to include `cli_status`, and add:

```rust
    #[test]
    fn the_command_line_row_says_where_it_stands() {
        assert_eq!(cli_status(false, false), "Comes with release builds on macOS");
        assert_eq!(cli_status(true, false), "Adds anywhere to your PATH, and a skill for Claude and Codex");
        assert_eq!(cli_status(true, true), "anywhere is on your PATH");
    }
```

**Step 2: Run it to verify it fails**

Run: `cd packages/desktop && cargo test -p pocket -- the_command_line_row`
Expected: FAIL to compile with `no function ... cli_status`.

**Step 3: Add the state.** In `host.rs`, add to `Host`:

```rust
    /// Whether the `anywhere` command points at this app, read when Settings opens.
    pub cli: bool,
```

Add `cli: false` to the struct literal in `Host::new`. In `load_host`, read it in the same background task:

```rust
            let (status, devices, cli) = cx.background_executor().spawn(async { (daemon::request(&json!({"op": "status"})), daemon::request(&json!({"op": "devices"})), daemon::cli::installed()) }).await;
            this.update(cx, |d, cx| {
                d.settings.host.cli = cli;
```

The rest of the closure stays as it is.

**Step 4: Add the row.** In `general.rs`:

Add the status text next to `update_status`:

```rust
fn cli_status(available: bool, installed: bool) -> &'static str {
    match (available, installed) {
        (false, _) => "Comes with release builds on macOS",
        (true, false) => "Adds anywhere to your PATH, and a skill for Claude and Codex",
        (true, true) => "anywhere is on your PATH",
    }
}
```

Add to `ROWS`, after the `"service"` row:

```rust
    Setting::custom("cli", "Startup & background", "Command line tool", |d, _, cx| d.cli_row(cx)).hint("Lets scripts and agents outside Anywhere run its sessions"),
```

Add to `impl Desktop`, after `service_row`:

```rust
    fn cli_row(&mut self, cx: &mut Context<Self>) -> Div {
        let available = cfg!(target_os = "macos") && channel::is_release();
        let installed = self.settings.host.cli;
        let title = div().flex().flex_col().gap(px(1.)).child("Command line tool").child(div().text_size(px(12.)).line_height(px(16.)).font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(cli_status(available, installed)));
        let install = (available && !installed).then(|| action("settings-install-cli", "Install").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.install_cli(cx))));
        row(title, None, div().when_some(install, |d, c| d.child(c)))
    }

    fn install_cli(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async { daemon::cli::install() }).await;
            this.update(cx, |d, cx| {
                match result {
                    Ok(()) => d.settings.host.cli = true,
                    Err(e) => d.error = Some(format!("Couldn't install the command line tool: {e}")),
                }
                cx.notify();
            })
        })
        .detach();
    }
```

**Step 5: Run the test to verify it passes**

Run: `cd packages/desktop && cargo test -p pocket -- the_command_line_row`
Expected: PASS.

**Step 6: Tell users.** In `README.md`, add after the Automations bullet under `## Features`:

```markdown
- **Scriptable.** Settings → General → Command line tool adds an `anywhere` command that starts, prompts and reads sessions, with a skill that teaches Claude Code and Codex to use it.
```

**Step 7: Run the desktop's full checks, and compare screens**

Run: `cd packages/desktop && cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: PASS, no new warnings.

Capture Settings → General before and after with `.ui-review/fixture/capture.sh`. Only the new row should differ.

---

## Verification

Automated tests use the fake claude, so these need a person, on a release build:

1. **Install.** Settings → General → Command line tool → Install. macOS asks for the password once. `which anywhere` prints `/usr/local/bin/anywhere`. `~/.claude/skills/anywhere/SKILL.md` and `~/.agents/skills/anywhere/SKILL.md` exist. The row now says it's installed, and stays so after reopening Settings.
2. **Cancel.** Remove the link, click Install, cancel the prompt. An error shows, the row still offers Install.
3. **Real claude.** From Terminal.app, in a project: `id=$(anywhere session create --prompt "Say hi in one word")`, `anywhere session wait "$id"`, `anywhere session read "$id"`. The session shows in the sidebar, and `read` prints the real reply.
4. **Real codex.** The same with `--provider codex`. The fake codex isn't covered by this plan's tests, and Codex's TUI reports through a different path. If `read` comes back empty, check how the timeline fills for a TUI codex before changing `settleFor`.
5. **A long real turn.** Run a prompt that takes a few tool calls, and check `read` returns the final reply. `settleFor` was only proven against the fake's instant transcript. If real claude writes its last lines later than 500ms after its Stop hook, raise it.
6. **On screen.** Keep the session open and focused while `wait` runs. The turn goes to Idle, and `wait` still prints `done`.
7. **Needs you.** Prompt something that asks permission. `wait` exits 4. Approve it in the app, `wait` again, it exits 0.
8. **Inside Anywhere.** In an Anywhere terminal, `anywhere session close x` says it can only read; `anywhere session list` works.
9. **Sandbox.** From a Claude Code session outside Anywhere with its sandbox on, run `anywhere session list`. If the sandbox blocks the socket, add that to the skill's refusal rule.
10. **pocketd stopped.** `anywhere session list` exits 1 and names the socket.

## Known limits, not fixed here

- **Reparent residual.** An agent inside a Terminal can `nohup` or `launchctl submit` itself out of its terminal and be taken for the owner (`docs/designs/2026-09-30-no-self-approval.md:299`). It exists today, but `anywhere` makes it far easier to use. Worth its own decision before this is advertised.
- **`--access full`** has no check; the skill's rule is only advice.
- **`settle`** is a wait, not a guarantee. A machine stalled for over 500ms can still lose the race. A real fix has the Stop hook drain the transcript before reporting the turn's end.
- **A failed turn on screen reads as done.** pocketd reports `Failed` only with status `done`, and a seen session goes to `idle`, so `wait` exits 0 for it.
- **Install has no in-flight guard.** A second click while macOS's password prompt is up queues another prompt.
- **`pickProject` takes the first project holding the cwd**, not the deepest, and `--project <relative path>` isn't resolved through symlinks.
- **macOS only.** Elsewhere the owner isn't known from the process, so the command would need an owner token.
