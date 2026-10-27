# Agent Shared Files Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** A Claude agent can share a file (image, document, log, …) with the user, and the desktop and phone chats show it as a card the user can open.

**Architecture:** pocketd's Claude plugin (`internal/daemon/plugin.go`) gains a `.mcp.json` that runs `pocketd mcp <home>/attachments`, a stdio MCP server with one tool, `share_file(path, caption)`. The tool copies the file into `<home>/attachments/<hash>-<name>` and answers with that path. The call lands in the transcript like any tool call (`detail.kind = "other"`, `detail.input = {path, caption}`, `output = kept path`); the desktop `chat` crate and the iOS `ChatModel` map it to a new `ToolKind::Shared` and a `Row::Shared` that stands outside phases, and each UI draws a card. The phone opens the copy with the existing `fs.read`, which now also serves the attachments folder.

**Toolset:**
- Go (from `packages/pocketd`): one package `go test ./internal/share/`; one test `go test ./internal/daemon/ -run TestWritePlugin`; all `go test ./...`; vet `go vet ./...`; build `go build -o bin/pocketd ./cmd/pocketd`.
- Rust (from `packages/desktop`): one crate's tests by name `cargo test -p chat shared`; `cargo test -p pocket shared`; before committing `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --workspace`.
- Swift (from `packages/ios`): `make test` (all AnywhereKit tests); one suite `cd Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter ChatModelTests`; iOS-only code `make typecheck-ios`.
- Screens (from repo root): `.ui-review/fixture/capture.sh <dir> <name>=<steps>…` with `FIXTURE_CHAT=<file in .ui-review/fixture/chat>`.

**Read first:**
- `CLAUDE.md` (repo root) — desktop layout rules (logic in model crates, one file per component, thin `impl Desktop`, tests named as sentences).
- `docs/adr/0003-desktop-code-layout.md` — where desktop code goes.
- `packages/pocketd/internal/daemon/plugin.go` — the plugin pocketd writes and loads into every Claude it starts (`CLAUDE_CODE_PLUGIN_DIRS`).
- `packages/desktop/crates/chat/src/rows.rs` — how `Row::Question` stands apart from phases; `Row::Shared` copies it.
- `packages/ios/Packages/AnywhereKit/Sources/ChatModel/Rows.swift` — the Swift port of `rows.rs`; it mirrors Rust line for line.

**Decisions (settled with the user):**
- v1 covers Claude on desktop and iOS. Codex isn't given the tool.
- The tool copies the file into pocketd's home (`<home>/attachments`), so it survives `/tmp` cleanup and later edits, and the phone can read it.
- Sharing never asks permission: chat Claudes start with `--allowedTools mcp__plugin_anywhere_anywhere__share_file`.
- Desktop: images show inline and open in the Explorer preview; text, code, markdown and SVG open in the Explorer preview; PDF, Office, iWork, audio, video, zip and HEIC open in the Mac's default app. For an agent on a remote host, every file opens in the Explorer preview.
- Phone: a card names the file and its caption, with no inline image. Tapping opens the existing file preview (images zoom); a document only an app can open isn't tappable.
- At most 20 MB per file (the most `fs.read` sends the phone).
- Out of scope: Codex, deleting old copies.

---

## PR 1: pocketd serves `share_file`

**Scope:** The `share` package (copy + MCP server), the `pocketd mcp` command, the plugin's `.mcp.json`, `--allowedTools` for chat Claudes, and `fs.read` over the attachments folder. Once merged, an agent can share a file, and chats show it as a plain MCP tool row (`share_file`) until PR 2 and PR 3 land.
**Depends on:** nothing
**Done when:** `go test ./...` is green and a real Claude started with pocketd's plugin calls `share_file` and gets back a path under `<home>/attachments`.

### Task 1.1: Keep a copy of a shared file

**Files:**
- Create: `packages/pocketd/internal/share/share.go`
- Test: `packages/pocketd/internal/share/share_test.go`

**Context:** `share` is a new package with no pocketd dependencies besides `internal/atomicfile` (`atomicfile.Write(path, data, perm)` writes via temp file + rename). `Keep` names the copy `<first 12 hex of sha256>-<base name>`, so sharing the same bytes again finds the existing copy, and a changed file gets a new one, which keeps old cards showing what they showed. The MCP server runs in the agent's working directory (Claude starts stdio servers there), so a relative path resolves against the process's cwd via `filepath.Abs`. Error messages are shown to both the agent and the user, so they're plain sentences.

**Step 1: Write the failing test**

```go
package share

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func write(t *testing.T, path, text string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(text), 0o644); err != nil {
		t.Fatal(err)
	}
}

// Proves the copy lands in dir under hash and name, holding the same bytes.
func TestKeepCopiesAFileUnderItsHashAndName(t *testing.T) {
	src := filepath.Join(t.TempDir(), "chart.png")
	write(t, src, "png bytes")
	dir := filepath.Join(t.TempDir(), "attachments")
	kept, err := Keep(dir, src)
	if err != nil {
		t.Fatal(err)
	}
	if filepath.Dir(kept) != dir || !strings.HasSuffix(kept, "-chart.png") || len(filepath.Base(kept)) != len("0123456789ab-chart.png") {
		t.Fatalf("kept = %q", kept)
	}
	if got, _ := os.ReadFile(kept); string(got) != "png bytes" {
		t.Fatalf("the copy holds %q", got)
	}
}

// Proves re-sharing is idempotent and an edit doesn't change what an earlier card shows.
func TestAFileSharedAgainLandsOnItsCopyAndAChangedOneOnANewCopy(t *testing.T) {
	src := filepath.Join(t.TempDir(), "notes.md")
	write(t, src, "# one")
	dir := t.TempDir()
	first, _ := Keep(dir, src)
	again, _ := Keep(dir, src)
	write(t, src, "# two")
	changed, _ := Keep(dir, src)
	if first != again || changed == first {
		t.Fatalf("first %q, again %q, changed %q", first, again, changed)
	}
	if got, _ := os.ReadFile(first); string(got) != "# one" {
		t.Fatalf("the first copy now holds %q", got)
	}
}

// Proves a relative path is read from the folder the server runs in.
func TestARelativePathIsFromTheFolderTheServerRunsIn(t *testing.T) {
	work := t.TempDir()
	write(t, filepath.Join(work, "notes.md"), "# hi")
	t.Chdir(work)
	kept, err := Keep(t.TempDir(), "notes.md")
	if err != nil || !strings.HasSuffix(kept, "-notes.md") {
		t.Fatalf("%q %v", kept, err)
	}
}

// Proves what can't be shared is refused with a reason the user can read.
func TestKeepRefusesWhatIsntAFileItCanShare(t *testing.T) {
	dir := t.TempDir()
	big := filepath.Join(dir, "big.bin")
	f, _ := os.Create(big)
	f.Truncate(Max + 1)
	f.Close()
	for path, want := range map[string]string{
		filepath.Join(dir, "gone.png"): "There's no file at",
		dir:                            "is a folder",
		big:                            "over 20 MB",
		"":                             "Name the file",
	} {
		if _, err := Keep(t.TempDir(), path); err == nil || !strings.Contains(err.Error(), want) {
			t.Errorf("%q: %v", path, err)
		}
	}
}
```

**Step 2: Run the test to verify it fails**

Run: `cd packages/pocketd && go test ./internal/share/`
Expected: FAIL to build with `undefined: Keep` and `undefined: Max`.

**Step 3: Write the implementation**

```go
// Package share is the MCP server pocketd's Claude plugin runs. Its one
// tool, share_file, keeps a copy of a file for the chat to show the user.
package share

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"

	"pocketd/internal/atomicfile"
)

// Max is the largest file a share keeps: the most fs.read sends the phone
// (fsview.ImageMax).
const Max = 20 << 20

// Keep copies the file at path into dir as <hash>-<name> and returns where.
// A relative path is from the folder the server runs in, the agent's. The
// same bytes shared again land on the copy kept before, so a card keeps
// showing what it showed when the file changes after.
func Keep(dir, path string) (string, error) {
	if path == "" {
		return "", errors.New("Name the file to share in path")
	}
	full, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	info, err := os.Stat(full)
	switch {
	case errors.Is(err, fs.ErrNotExist):
		return "", fmt.Errorf("There's no file at %s", path)
	case err != nil:
		return "", err
	case info.IsDir():
		return "", fmt.Errorf("%s is a folder; share a file in it", path)
	case info.Size() > Max:
		return "", errors.New("Files over 20 MB can't be shared")
	}
	data, err := os.ReadFile(full)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(data)
	kept := filepath.Join(dir, hex.EncodeToString(sum[:6])+"-"+filepath.Base(full))
	if _, err := os.Stat(kept); err == nil {
		return kept, nil
	}
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	if err := atomicfile.Write(kept, data, 0o600); err != nil {
		return "", err
	}
	return kept, nil
}
```

**Step 4: Run the test to verify it passes**

Run: `cd packages/pocketd && go test ./internal/share/`
Expected: PASS (`ok  pocketd/internal/share`)

### Task 1.2: Serve `share_file` over MCP

**Files:**
- Modify: `packages/pocketd/internal/share/share.go` (append)
- Test: `packages/pocketd/internal/share/serve_test.go`

**Context:** MCP over stdio is JSON-RPC 2.0, one JSON object per line. Claude sends `initialize` (answer with the `protocolVersion` it asked for, `capabilities.tools`, `serverInfo`), then the notification `notifications/initialized` (it has no `id`, so it gets no reply), then `tools/list` and `tools/call`. A tool failure is a normal result with `isError: true` and the reason as text; an unknown method is a JSON-RPC error `-32601`. Claude names a plugin's MCP tools `mcp__plugin_<plugin>_<server>__<tool>`; the plugin is `anywhere` (`plugin.json`) and the server `anywhere` (Task 1.3), so the tool is `mcp__plugin_anywhere_anywhere__share_file`. `Tool` holds that name for `--allowedTools` (Task 1.4); the desktop and phone hold the same string. The answer text is the kept path alone, since that's what the UIs read from the call's output.

**Step 1: Write the failing test**

```go
package share

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type reply struct {
	ID     int `json:"id"`
	Result struct {
		ProtocolVersion string `json:"protocolVersion"`
		Tools           []struct {
			Name string `json:"name"`
		} `json:"tools"`
		Content []struct {
			Type string `json:"type"`
			Text string `json:"text"`
		} `json:"content"`
		IsError bool `json:"isError"`
	} `json:"result"`
	Error *struct {
		Code int `json:"code"`
	} `json:"error"`
}

func serve(t *testing.T, dir string, lines ...string) []reply {
	t.Helper()
	var out bytes.Buffer
	if err := Serve(strings.NewReader(strings.Join(lines, "\n")), &out, dir); err != nil {
		t.Fatal(err)
	}
	var replies []reply
	for dec := json.NewDecoder(&out); dec.More(); {
		var r reply
		if err := dec.Decode(&r); err != nil {
			t.Fatal(err)
		}
		replies = append(replies, r)
	}
	return replies
}

// Proves the handshake a Claude does, and that notifications get no reply.
func TestServeAnswersTheHandshakeAndListsShareFile(t *testing.T) {
	got := serve(t, t.TempDir(),
		`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-code","version":"2"}}}`,
		`{"jsonrpc":"2.0","method":"notifications/initialized"}`,
		`{"jsonrpc":"2.0","id":2,"method":"tools/list"}`,
		`{"jsonrpc":"2.0","id":3,"method":"resources/list"}`)
	if len(got) != 3 {
		t.Fatalf("%d replies; the notification gets none: %+v", len(got), got)
	}
	if got[0].ID != 1 || got[0].Result.ProtocolVersion != "2025-06-18" {
		t.Errorf("initialize = %+v", got[0])
	}
	if len(got[1].Result.Tools) != 1 || got[1].Result.Tools[0].Name != "share_file" {
		t.Errorf("tools = %+v", got[1].Result.Tools)
	}
	if got[2].Error == nil || got[2].Error.Code != -32601 {
		t.Errorf("an unknown method = %+v", got[2])
	}
}

// Proves a call answers the kept path, and a failed share says why as a tool error.
func TestACallKeepsTheFileAndAnswersWhereOrWhyNot(t *testing.T) {
	src := filepath.Join(t.TempDir(), "report.pdf")
	os.WriteFile(src, []byte("%PDF"), 0o644)
	dir := t.TempDir()
	got := serve(t, dir,
		`{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"share_file","arguments":{"path":"`+src+`","caption":"Q3"}}}`,
		`{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"share_file","arguments":{"path":"/nope/x.png"}}}`)
	ok, failed := got[0].Result, got[1].Result
	if ok.IsError || len(ok.Content) != 1 || ok.Content[0].Type != "text" || filepath.Dir(ok.Content[0].Text) != dir || !strings.HasSuffix(ok.Content[0].Text, "-report.pdf") {
		t.Errorf("share = %+v", ok)
	}
	if !failed.IsError || !strings.Contains(failed.Content[0].Text, "There's no file at /nope/x.png") {
		t.Errorf("failed share = %+v", failed)
	}
}

// Proves the tool name the launcher allows is the one Claude gives the plugin's tool.
func TestToolIsWhatClaudeNamesThePluginsShareFile(t *testing.T) {
	if Tool != "mcp__plugin_anywhere_anywhere__share_file" {
		t.Fatal(Tool)
	}
}
```

**Step 2: Run the test to verify it fails**

Run: `cd packages/pocketd && go test ./internal/share/`
Expected: FAIL to build with `undefined: Serve` and `undefined: Tool`.

**Step 3: Write the implementation**

Add `"encoding/json"` and `"io"` to the imports of `share.go`, then append:

```go
// Tool is what Claude names share_file: the tool of the anywhere server in
// pocketd's anywhere plugin.
const Tool = "mcp__plugin_anywhere_anywhere__share_file"

const latest = "2025-06-18"

var shareFile = map[string]any{
	"name": "share_file",
	"description": "Show the user a file: an image, chart, screenshot, document, log or report you made or found. " +
		"It appears as a card in their chat, on their Mac and their phone, and opens from there. " +
		"Anywhere keeps a copy, so share a file once it's final. Returns the path of that copy.",
	"inputSchema": map[string]any{
		"type": "object",
		"properties": map[string]any{
			"path":    map[string]any{"type": "string", "description": "The file to share: absolute, or relative to your working directory."},
			"caption": map[string]any{"type": "string", "description": "One line on what it is, shown with it."},
		},
		"required": []string{"path"},
	},
}

type request struct {
	ID     json.RawMessage `json:"id"`
	Method string          `json:"method"`
	Params json.RawMessage `json:"params"`
}

type response struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  any             `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

// Serve answers MCP requests from r on w, a JSON message a line, until r
// ends. share_file keeps its copies in dir.
func Serve(r io.Reader, w io.Writer, dir string) error {
	dec, enc := json.NewDecoder(r), json.NewEncoder(w)
	for {
		var req request
		if err := dec.Decode(&req); errors.Is(err, io.EOF) {
			return nil
		} else if err != nil {
			return err
		}
		// A notification wants no answer.
		if req.ID == nil {
			continue
		}
		res := response{JSONRPC: "2.0", ID: req.ID}
		res.Result, res.Error = answer(req, dir)
		if err := enc.Encode(res); err != nil {
			return err
		}
	}
}

func answer(req request, dir string) (any, *rpcError) {
	switch req.Method {
	case "initialize":
		var p struct {
			ProtocolVersion string `json:"protocolVersion"`
		}
		json.Unmarshal(req.Params, &p)
		if p.ProtocolVersion == "" {
			p.ProtocolVersion = latest
		}
		return map[string]any{
			"protocolVersion": p.ProtocolVersion,
			"capabilities":    map[string]any{"tools": map[string]any{}},
			"serverInfo":      map[string]string{"name": "anywhere", "version": "1.0.0"},
		}, nil
	case "ping":
		return map[string]any{}, nil
	case "tools/list":
		return map[string]any{"tools": []any{shareFile}}, nil
	case "tools/call":
		var p struct {
			Name      string `json:"name"`
			Arguments struct {
				Path string `json:"path"`
			} `json:"arguments"`
		}
		if json.Unmarshal(req.Params, &p) != nil || p.Name != "share_file" {
			return nil, &rpcError{-32602, "No such tool"}
		}
		kept, err := Keep(dir, p.Arguments.Path)
		if err != nil {
			return said(err.Error(), true), nil
		}
		return said(kept, false), nil
	}
	return nil, &rpcError{-32601, "No such method: " + req.Method}
}

// said is a tool's answer: text, and whether it reports a failure.
func said(text string, failed bool) map[string]any {
	return map[string]any{"content": []any{map[string]string{"type": "text", "text": text}}, "isError": failed}
}
```

**Step 4: Run the test to verify it passes**

Run: `cd packages/pocketd && go test ./internal/share/ && go vet ./internal/share/`
Expected: PASS, no vet output.

### Task 1.3: `pocketd mcp` and the plugin's `.mcp.json`

**Files:**
- Modify: `packages/pocketd/cmd/pocketd/main.go:16` (usage) and the `switch` at `:34-80`
- Modify: `packages/pocketd/internal/daemon/plugin.go:41-44` (`files` map)
- Test: `packages/pocketd/internal/daemon/plugin_test.go` (append)

**Context:** `WritePlugin` writes `<home>/plugin` on every `pocketd serve` (`cmd/pocketd/serve.go:152`), and `Env` puts that folder in `CLAUDE_CODE_PLUGIN_DIRS` for every terminal, so every Claude pocketd starts loads the plugin. Claude reads a plugin's MCP servers from `.mcp.json` at its root. `d.Exe` is pocketd's own absolute path (the hooks run it too). The attachments folder is `filepath.Join(d.Home, "attachments")`, the same folder `serve.go:216` gives `fsview.Service.Attachments`, so the phone can read what's kept there (Task 1.5). `.mcp.json` takes `command` and `args` separately, so the path needs no shell quoting.

**Step 1: Write the failing test**

Append to `plugin_test.go`:

```go
// Proves every Claude pocketd starts gets the share_file server, keeping copies where fs.read serves them.
func TestWritePluginServesShareFile(t *testing.T) {
	d := newDaemon(t)
	if err := d.WritePlugin(); err != nil {
		t.Fatal(err)
	}
	var file struct {
		McpServers map[string]struct {
			Command string
			Args    []string
		} `json:"mcpServers"`
	}
	readJSON(t, filepath.Join(d.Plugin, ".mcp.json"), &file)
	s, ok := file.McpServers["anywhere"]
	if len(file.McpServers) != 1 || !ok || s.Command != d.Exe || !slices.Equal(s.Args, []string{"mcp", filepath.Join(d.Home, "attachments")}) {
		t.Fatalf("mcpServers = %+v", file.McpServers)
	}
}
```

**Step 2: Run the test to verify it fails**

Run: `cd packages/pocketd && go test ./internal/daemon/ -run TestWritePluginServesShareFile`
Expected: FAIL with `open …/plugin/.mcp.json: no such file or directory`.

**Step 3: Write the implementation**

In `plugin.go`, add the `.mcp.json` entry to `files`:

```go
	files := map[string]any{
		".claude-plugin/plugin.json": map[string]string{"name": "anywhere", "version": "1.0.0", "description": "Shows Claude sessions in Anywhere."},
		"hooks/hooks.json":           map[string]any{"hooks": hooks},
		// share_file, which shows the user a file in the chat.
		".mcp.json": map[string]any{"mcpServers": map[string]any{
			"anywhere": map[string]any{"command": d.Exe, "args": []string{"mcp", filepath.Join(d.Home, "attachments")}},
		}},
	}
```

In `cmd/pocketd/main.go`, add `"pocketd/internal/share"` to the imports, add `| mcp <dir>` to `usage` right after `| hook`, and add this case to the `switch`, right after `case os.Args[1] == "hook":`'s body:

```go
	case os.Args[1] == "mcp" && len(os.Args) == 3:
		err = share.Serve(os.Stdin, os.Stdout, os.Args[2])
```

**Step 4: Run the test to verify it passes**

Run: `cd packages/pocketd && go test ./internal/daemon/ -run TestWritePlugin && go build -o bin/pocketd ./cmd/pocketd && printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | bin/pocketd mcp /tmp/aw-share`
Expected: PASS, then one JSON line whose `result.tools[0].name` is `"share_file"`.

### Task 1.4: Chat Claudes share without asking

**Files:**
- Modify: `packages/pocketd/internal/launch/claude.go:27-31`
- Modify: `packages/pocketd/internal/launch/argv_test.go`, `packages/pocketd/internal/launch/resume_test.go` (expected argv)

**Context:** `chatFlags` is appended for a chat on launch (`claude.go:53`) and on resume (`claude.go:98`), so one change covers both. `--allowedTools` is variadic in Claude's CLI; every flag pocketd appends after `chatFlags` starts with `-`, so it takes only the one tool name. Only `argv_test.go` and `resume_test.go` spell out the chat flags (`grep -rl -- --permission-prompt-tool packages` lists them and `claude.go`).

**Step 1: Update the tests to expect the flag**

Run from `packages/pocketd`:

```sh
sed -i '' 's/--permission-prompt-tool stdio --allow-dangerously-skip-permissions/--permission-prompt-tool stdio --allow-dangerously-skip-permissions --allowedTools mcp__plugin_anywhere_anywhere__share_file/g' internal/launch/argv_test.go internal/launch/resume_test.go
sed -i '' 's/"stdio", "--allow-dangerously-skip-permissions",/"stdio", "--allow-dangerously-skip-permissions", "--allowedTools", "mcp__plugin_anywhere_anywhere__share_file",/g' internal/launch/argv_test.go internal/launch/resume_test.go
grep -c allowedTools internal/launch/argv_test.go internal/launch/resume_test.go
```

Expected: `argv_test.go:8` and `resume_test.go:1` (lines 38-43, 100 and 186 of `argv_test.go`; line 148 of `resume_test.go`).

**Step 2: Run the tests to verify they fail**

Run: `cd packages/pocketd && go test ./internal/launch/`
Expected: FAIL in `TestClaudeRunsForTheChatAndLeavesItsPromptOut` and the table tests, `got` lacking `--allowedTools`.

**Step 3: Write the implementation**

Add `"pocketd/internal/share"` to `claude.go`'s imports and replace the `chatFlags` comment and declaration:

```go
// chatFlags run claude for the chat: stream-json both ways, its asks to use a
// tool as control requests rather than a dialog, Full allowed so the chat
// can switch to it whatever it launched or resumed at, its text as it
// streams, which the transcript holds only once a block is whole, and
// share_file without asking: it only copies a file the agent can read into
// pocketd's home for the user to see.
var chatFlags = []string{"-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--permission-prompt-tool", "stdio", allowBypass, "--allowedTools", share.Tool}
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/pocketd && go test ./internal/launch/`
Expected: PASS

### Task 1.5: `fs.read` serves the attachments folder

**Files:**
- Modify: `packages/pocketd/internal/fsview/service.go:108-115` (`Resolve`)
- Test: `packages/pocketd/internal/fsview/service_test.go` (append)

**Context:** The phone opens files with `fs.read{tree, path}`. `Resolve` accepts only a tree inside a registered project (`Tree` in `tree.go`), so a kept copy in `<home>/attachments` is refused today. The phone will ask with `tree = <home>/attachments` and `path = <hash>-<name>` (it splits the kept path at its last `/`). `Join` already refuses `..`, absolute paths and links out of the root, so allowing that one folder exposes nothing else. The folder also holds the images the phone uploaded, which are the user's own. The `repo` test helper gives `s.Attachments = <base>/home/attachments` with symlinks resolved; `put(t, root, rel, text)` writes a file; `codeOf(err)` reads a refusal's code.

**Step 1: Write the failing test**

```go
// Proves the phone can open what an agent shared, and only that folder's files.
func TestAReadOfTheAttachmentsFolderServesWhatAnAgentShared(t *testing.T) {
	s, _ := repo(t)
	put(t, s.Attachments, "0a1b2c3d4e5f-chart.png", "\x89PNG\r\n\x1a\n")
	r, err := s.Read(s.Attachments, "0a1b2c3d4e5f-chart.png", 0)
	if err != nil || r.Kind != "image" || r.Tree != s.Attachments || r.Path != "0a1b2c3d4e5f-chart.png" {
		t.Fatalf("%+v %v", r, err)
	}
	if _, err := s.Read(s.Attachments, "../../desktop.json", 0); codeOf(err) != proto.CodeOutsideTree {
		t.Fatalf("a path out of the folder: %v", err)
	}
	if _, err := s.Read(filepath.Dir(s.Attachments), "attachments/0a1b2c3d4e5f-chart.png", 0); codeOf(err) != proto.CodeNotATree {
		t.Fatalf("pocketd's home: %v", err)
	}
}
```

**Step 2: Run the test to verify it fails**

Run: `cd packages/pocketd && go test ./internal/fsview/ -run TestAReadOfTheAttachmentsFolder`
Expected: FAIL with `{…} This folder isn't a project on your Mac` (a not-a-tree refusal).

**Step 3: Write the implementation**

Replace `Resolve`:

```go
// Resolve is tree's real root and rel's real path inside it. A tree is a
// project's worktree, or the attachments folder, so the phone can open what
// an agent shared.
func (s *Service) Resolve(tree, rel string) (root, full string, err error) {
	if s.Attachments != "" && tree == s.Attachments {
		full, err = Join(tree, rel)
		return tree, full, err
	}
	if root, err = Tree(s.Registry.Load(), tree); err != nil {
		return "", "", err
	}
	full, err = Join(root, rel)
	return root, full, err
}
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/pocketd && go test ./internal/fsview/ && go test ./... && go vet ./...`
Expected: PASS everywhere, no vet output.

### Task 1.6: Check it against a real Claude

**Files:** none (manual check).

**Context:** Two facts this PR rests on can only be checked against a real Claude: that it loads `.mcp.json` from a plugin folder given in `CLAUDE_CODE_PLUGIN_DIRS`, and that it names the tool `mcp__plugin_anywhere_anywhere__share_file`. If the name differs, change `share.Tool`, the test in Task 1.2 and the expectations from Task 1.4 to the real name, and note it for PR 2 and PR 3, which hold the same string.

**Step 1: Install the dev pocketd and let it rewrite its plugin**

Run from the repo root: `make pocketd && launchctl kickstart -k gui/$(id -u)/dev.mingo.anywhere.dev.pocketd && sleep 2 && cat ~/.coding-pocket-dev/plugin/.mcp.json`
Expected: JSON naming `packages/pocketd/bin/pocketd` (absolute) with args `["mcp", "<home>/.coding-pocket-dev/attachments"]`.

**Step 2: Have a Claude share a file**

```sh
mkdir -p /tmp/aw-share-src && echo '# hello' > /tmp/aw-share-src/hello.md
CLAUDE_CODE_PLUGIN_DIRS=$HOME/.coding-pocket-dev/plugin claude -p --allowedTools mcp__plugin_anywhere_anywhere__share_file \
  "Call the share_file tool with path /tmp/aw-share-src/hello.md and caption 'test'. Then print only the exact tool name you called and what it returned."
```

Expected: it prints `mcp__plugin_anywhere_anywhere__share_file` and a path like `~/.coding-pocket-dev/attachments/<12 hex>-hello.md`; `cat` of that path prints `# hello`. No permission prompt.

**Step 3: See it in the app**

Run `make app`, open Anywhere Dev, start a chat with Claude and ask it to share `/tmp/aw-share-src/hello.md`. Expected: the reply holds a `share_file` tool row whose output is the kept path.

---

## PR 2: The desktop chat shows shared files

**Scope:** `ToolKind::Shared`, the `chat::shared` module, `Row::Shared`, the card view and opening a shared file. Works on its own (the tool name is a constant), and shows real cards once PR 1 is in.
**Depends on:** nothing to merge; PR 1 to see it with a real agent.
**Done when:** `cargo test --workspace` is green, clippy has no new warnings, and the capture of `FIXTURE_CHAT=shared-files` shows an inline image card, a PDF card, a markdown card, a "Sharing…" card and a failed one, all visible with the reply's fold shut.

### Task 2.1: `ToolKind::Shared` and how a shared file shows

**Files:**
- Create: `packages/desktop/crates/chat/src/shared.rs`
- Modify: `packages/desktop/crates/chat/src/chat.rs:4-13` (modules) and `:92-103` (`ToolKind`)
- Modify: `packages/desktop/crates/pocket/src/chat/permission.rs:13-24` (`subject`)
- Modify: `packages/desktop/crates/pocket/src/chat/tool_row.rs:72-89` (`line`)

**Context:** The `chat` crate is a model crate: no GPUI. `shared.rs` holds every decision the card needs as free functions over plain data, so they're testable. `kept` reads the copy's path from the call's output (Task 1.2 answers with the path alone). `shown` decides by extension; on a remote host (`hosts::split(cwd).0.is_some()` in the desktop), `img()` can't read the path and no app on this Mac opens it, so everything goes to the Explorer preview, which reads host files. The two `pocket` matches over `ToolKind` are exhaustive, so they need an arm for the new variant to compile; the permission arm is never shown in practice (the tool is pre-allowed) but must exist.

**Step 1: Write the failing test**

Create `shared.rs` with only its tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolKind;

    fn share(status: ToolStatus, caption: &str, output: Option<&str>) -> ToolCall {
        let kind = ToolKind::Shared { path: "out/chart.png".into(), caption: caption.into() };
        ToolCall { id: "t".into(), name: TOOL.into(), kind, status, output: output.map(Into::into), duration_ms: None, at: 0 }
    }

    #[test]
    fn images_show_inline_documents_open_in_their_app_and_the_rest_in_the_preview() {
        assert_eq!(shown("/r/out/Chart.PNG", false), Shown::Image);
        assert_eq!(shown("/r/logo.svg", false), Shown::Image);
        assert_eq!(shown("/r/report.pdf", false), Shown::App);
        assert_eq!(shown("/r/deck.KEY", false), Shown::App);
        assert_eq!(shown("/r/notes.md", false), Shown::Preview);
        assert_eq!(shown("/r/build.log", false), Shown::Preview);
        assert_eq!(shown("/r.d/Makefile", false), Shown::Preview, "a dot in a folder's name isn't an extension");
    }

    #[test]
    fn a_file_on_a_host_always_opens_in_the_preview() {
        assert_eq!(shown("/srv/chart.png", true), Shown::Preview);
        assert_eq!(shown("/srv/report.pdf", true), Shown::Preview);
    }

    #[test]
    fn only_a_share_that_went_through_has_a_kept_copy() {
        assert_eq!(kept(&share(ToolStatus::Ok, "", Some("/h/attachments/0a1b-chart.png\n"))), Some("/h/attachments/0a1b-chart.png"));
        assert_eq!(kept(&share(ToolStatus::Running, "", None)), None);
        assert_eq!(kept(&share(ToolStatus::Failed, "", Some("There's no file at out/chart.png"))), None);
        assert_eq!(kept(&share(ToolStatus::Ok, "", Some(" "))), None);
    }

    #[test]
    fn a_card_reads_the_caption_that_it_is_on_its_way_or_why_it_failed() {
        assert_eq!(detail(&share(ToolStatus::Ok, "Q3  revenue\nby month", Some("/k"))), "Q3 revenue by month");
        assert_eq!(detail(&share(ToolStatus::Running, "Q3", None)), "Sharing…");
        assert_eq!(detail(&share(ToolStatus::Failed, "Q3", Some("Files over 20 MB can't be shared\n"))), "Files over 20 MB can't be shared");
        assert_eq!(detail(&share(ToolStatus::Failed, "Q3", None)), "Couldn't share it");
    }

    #[test]
    fn a_card_names_the_file_by_its_own_name() {
        assert_eq!(name("/r/out/chart.png"), "chart.png");
        assert_eq!(name(r"C:\r\chart.png"), "chart.png");
        assert_eq!(name("chart.png"), "chart.png");
    }
}
```

Add `pub mod shared;` to `chat.rs` after `mod session;`.

**Step 2: Run the test to verify it fails**

Run: `cd packages/desktop && cargo test -p chat shared`
Expected: FAIL to compile: `cannot find function shown`, `no variant named Shared`, etc.

**Step 3: Write the implementation**

In `chat.rs`, add to `ToolKind`, right after `Question(Vec<Question>),`:

```rust
    /// A file the agent shared through Anywhere's `share_file`: the path it named and its caption. The copy the user
    /// sees is the call's output (`shared::kept`).
    Shared { path: String, caption: String },
```

Put this above the tests in `shared.rs`:

```rust
//! Files an agent shares with the user through Anywhere's `share_file` tool, and how the chat shows them.

use crate::{ToolCall, ToolKind, ToolStatus};

/// What Claude names `share_file`: the tool of the `anywhere` server in pocketd's `anywhere` plugin.
pub const TOOL: &str = "mcp__plugin_anywhere_anywhere__share_file";

/// How a shared file shows in the chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// Inline, opening in the preview.
    Image,
    /// As a card that opens in the preview.
    Preview,
    /// As a card that opens in the Mac's app for it.
    App,
}

const IMAGE: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg"];
/// Files the preview can't show, which the Mac opens in their own app.
const APP: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "key", "pages", "numbers", "rtf", "heic", "mp4", "mov", "m4v", "webm", "mp3",
    "wav", "m4a", "aac", "zip",
];

/// How the file at `path` shows. A file on a host goes to the preview: `img` reads only this Mac's paths, and no app
/// here opens one there.
pub fn shown(path: &str, remote: bool) -> Shown {
    let ext = name(path).rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        _ if remote => Shown::Preview,
        e if IMAGE.contains(&e) => Shown::Image,
        e if APP.contains(&e) => Shown::App,
        _ => Shown::Preview,
    }
}

/// Where Anywhere keeps the copy the user sees: what the tool answered, once the share went through.
pub fn kept(call: &ToolCall) -> Option<&str> {
    let out = call.output.as_deref()?.trim();
    (call.status == ToolStatus::Ok && !out.is_empty()).then_some(out)
}

/// What the card reads after the file's name: its caption, that it's on its way, or why it didn't go through.
pub fn detail(call: &ToolCall) -> String {
    let caption = match &call.kind {
        ToolKind::Shared { caption, .. } => caption.as_str(),
        _ => "",
    };
    match call.status {
        ToolStatus::Running => "Sharing…".into(),
        ToolStatus::Failed => call.output.as_deref().map(str::trim).filter(|o| !o.is_empty()).unwrap_or("Couldn't share it").into(),
        ToolStatus::Ok => caption.split_whitespace().collect::<Vec<_>>().join(" "),
    }
}

/// The file's own name, as the card shows it.
pub fn name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
```

In `permission.rs` `subject`, add before the `Question | Other` arm:

```rust
        ToolKind::Shared { path, .. } => ("file", "Share", path),
```

In `tool_row.rs` `line`, add before `ToolKind::Question(_)`:

```rust
        ToolKind::Shared { path, .. } => ("Share", Arg::File(path.clone())),
```

**Step 4: Run the test to verify it passes**

Run: `cd packages/desktop && cargo test -p chat shared && cargo build --workspace`
Expected: 5 tests PASS; the build succeeds. If it names another non-exhaustive `match` on `ToolKind`, add a `ToolKind::Shared { .. }` arm there worded like the `Question` arm beside it.

### Task 2.2: The timeline maps `share_file` to `ToolKind::Shared`

**Files:**
- Modify: `packages/desktop/crates/chat/src/timeline.rs:16` (doc) and `:230-245` (`kind`)
- Test: `packages/desktop/crates/chat/src/timeline.rs` (tests module)

**Context:** pocketd reports an MCP call as `detail.kind = "other"` with `detail.input` = the tool's input. `kind()` already special-cases `AskUserQuestion`, `WebFetch` and `WebSearch` by name; `share_file` goes beside them. Test helpers in the module: `items(json!([...]))`, `tool(id, name, detail, status)`, `only_tool(&t)`.

**Step 1: Write the failing test**

Add to the tests module:

```rust
    #[test]
    fn a_share_file_call_becomes_a_shared_file() {
        let detail = json!({"kind": "other", "name": crate::shared::TOOL, "input": {"path": "out/chart.png", "caption": "Q3 revenue"}});
        let t = fold(&items(json!([tool("sh", crate::shared::TOOL, detail, "ok")])));
        assert_eq!(only_tool(&t).kind, ToolKind::Shared { path: "out/chart.png".into(), caption: "Q3 revenue".into() });
    }
```

**Step 2: Run the test to verify it fails**

Run: `cd packages/desktop && cargo test -p chat a_share_file_call`
Expected: FAIL: `left: Other { input: "{\n  \"caption\": …`, `right: Shared { … }`.

**Step 3: Write the implementation**

In `kind`, add after the `AskUserQuestion` arm:

```rust
        _ if name == crate::shared::TOOL => ToolKind::Shared { path: input_text(&d.input, "path"), caption: input_text(&d.input, "caption") },
```

Change the doc line `/// - AskUserQuestion → `ToolKind::Question`.` to:

```rust
/// - AskUserQuestion → `ToolKind::Question`; `share_file` → `ToolKind::Shared`.
```

**Step 4: Run the test to verify it passes**

Run: `cd packages/desktop && cargo test -p chat`
Expected: PASS

### Task 2.3: A shared file is a row of its own

**Files:**
- Modify: `packages/desktop/crates/chat/src/rows.rs` (`Row` `:6-30`, `id` `:42`, `turn` `:52-66`, `plain` `:126-134`, `is_work` `:142-148`, `top_gap` `:321`)
- Modify: `packages/desktop/crates/chat/src/session.rs:112` (`same_content`)
- Modify: `packages/desktop/crates/pocket/src/chat/transcript.rs:740` (temporary arm, replaced in Task 2.4)
- Test: `packages/desktop/crates/chat/src/rows.rs` (tests module)

**Context:** A phase is folded work that a settled reply hides. A shared file must stay visible, so, like an AskUserQuestion call, it isn't work (`is_work`); it ends the phase before it and gets its own `Row`. `plain` lays out a block's own row outside phases. In the rows tests, `call(kind, status)` makes a tool block, `shell()` a shell step and `one(blocks)` a one-turn transcript. Adding a `Row` variant breaks the exhaustive `match &row` in `pocket/src/chat/transcript.rs` (`chat_row`); give it a placeholder arm here so the workspace builds, and Task 2.4 replaces it with the card.

**Step 1: Write the failing test**

Add to the rows tests module, beside `a_question_ends_the_phase_and_stands_alone`:

```rust
    #[test]
    fn a_shared_file_ends_the_phase_and_stands_alone() {
        let shared = call(ToolKind::Shared { path: "chart.png".into(), caption: String::new() }, ToolStatus::Ok);
        let t = one(vec![shell(), shared, shell()]);
        assert_eq!(rows(&t, 0, false)[2..5], [Row::Phase { turn: 0, blocks: 0..1 }, Row::Shared { turn: 0, block: 1 }, Row::Phase { turn: 0, blocks: 2..3 }]);
    }
```

**Step 2: Run the test to verify it fails**

Run: `cd packages/desktop && cargo test -p chat a_shared_file`
Expected: FAIL to compile: `no variant named Shared found for enum Row`.

**Step 3: Write the implementation**

In `rows.rs`:

1. Add to `Row`, after `Question { turn: usize, block: usize },`:

```rust
    /// A file the agent shared: a card that, like a question, ends the phase before it.
    Shared { turn: usize, block: usize },
```

2. In `id`, extend the arm at `:42`: `Row::Tasks { turn: ix, block } | Row::Question { turn: ix, block } | Row::Shared { turn: ix, block } | Row::Compacted { turn: ix, block } | Row::Note { turn: ix, block } => {`

3. In `turn`, add `| Row::Shared { turn, .. }` after `| Row::Question { turn, .. }`.

4. In `plain`, add after the `Question` arm:

```rust
        Block::Tool(c) if matches!(c.kind, ToolKind::Shared { .. }) => rows.push(Row::Shared { turn, block: b }),
```

5. Replace `is_work`:

```rust
/// A step of a phase: thinking, a tool call or a sub-agent, but not a question or a shared file, which stand apart.
pub fn is_work(b: &Block) -> bool {
    match b {
        Block::Thinking(_) => true,
        Block::Tool(c) => !matches!(c.kind, ToolKind::Question(_) | ToolKind::Shared { .. }),
        _ => false,
    }
}
```

6. In `top_gap`, change `:321` to `(_, Row::Question { .. } | Row::Shared { .. } | Row::Compacted { .. } | Row::Notice { .. } | Row::Note { .. }) => 8.,`

In `session.rs:112`, extend the arm: `Row::Tasks { block, .. } | Row::Question { block, .. } | Row::Shared { block, .. } | Row::Compacted { block, .. } | Row::Note { block, .. } => {`

In `pocket/src/chat/transcript.rs`, right before the `Row::Question { turn, block } =>` arm of `chat_row`, add the placeholder:

```rust
            Row::Shared { .. } => div(),
```

**Step 4: Run the test to verify it passes**

Run: `cd packages/desktop && cargo test -p chat && cargo build --workspace`
Expected: PASS; the build succeeds.

### Task 2.4: The shared-file card, and opening it

**Files:**
- Create: `packages/desktop/crates/pocket/src/chat/shared_card.rs`
- Modify: `packages/desktop/crates/pocket/src/chat.rs:1-28` (add `mod shared_card;` in order, after `mod shimmer;`)
- Modify: `packages/desktop/crates/pocket/src/chat/transcript.rs` (`chat_row` at `:604`, the placeholder arm from Task 2.3, imports `:8`, tests at `:844`)
- Modify: `packages/desktop/crates/pocket/src/chat/locate.rs` (add `chat_open_shared` after `chat_open_file`, `:48-75`)

**Context:** One file per component. The card mirrors `question_marker.rs` (rounded 10 pt card, `hairline(0.08)` border, `ink(0.045)` fill, a 34 pt line, `TEXT_SIZE` from `tool_detail`). An image shows above the line, at most 360×240 and never wider than the chat, read from the kept path with `img(PathBuf)` as `user_bubble.rs:175` reads pasted images. A card opens only once the share went through (`kept` is `Some`). `impl Desktop` stays thin: `chat_open_shared` only routes to `cx.open_with_system` (as the Explorer header's "open in editor" button does, `explorer/preview/header.rs:74`) or to `open_doc(Doc::File(..))` (as `chat_open_file` does). For an agent on a host, its `cwd` is a `//<host>/…` key; the kept path is on that host, so the desktop key is `hosts::place(host, kept)`. `shown(t, row, fold_open)` in `transcript.rs` already shows every row except phases and todos under a shut fold; the test pins that for `Row::Shared`.

**Step 1: Write the failing test**

Add to the tests module at the bottom of `transcript.rs`:

```rust
    #[test]
    fn a_shared_file_shows_under_a_shut_fold() {
        let call = ToolCall {
            id: "s".into(),
            name: chat::shared::TOOL.into(),
            kind: ToolKind::Shared { path: "chart.png".into(), caption: String::new() },
            status: ToolStatus::Ok,
            output: Some("/h/attachments/0a1b-chart.png".into()),
            duration_ms: None,
            at: 0,
        };
        let t = Transcript { turns: vec![Turn { id: "u1".into(), blocks: vec![Block::Tool(call)], ..Turn::default() }] };
        assert!(shown(&t, &Row::Shared { turn: 0, block: 0 }, false));
    }
```

**Step 2: Run the test to verify it passes already**

Run: `cd packages/desktop && cargo test -p pocket a_shared_file_shows`
Expected: PASS. It pins the behaviour the card relies on, so a later change to `shown` can't hide shared files. The card itself is a view, and views get no render tests (CLAUDE.md); Task 2.5 checks it on screen.

**Step 3: Write the implementation**

`shared_card.rs`:

```rust
use super::tool_detail::TEXT_SIZE;
use chat::shared::{self, Shown};
use chat::{ToolCall, ToolKind, ToolStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;
use theme::*;

const IMAGE_W: f32 = 360.;
const IMAGE_H: f32 = 240.;

/// A file the agent shared: an image inline over a line naming it, any other file as the line alone. Once the share
/// went through, `kept` is its copy and a click opens it, by `on_click`.
pub(crate) fn card(id: &str, call: &ToolCall, shown: Shown, kept: Option<&str>, width: f32, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    let path = match &call.kind {
        ToolKind::Shared { path, .. } => path.as_str(),
        _ => "",
    };
    let image = kept.filter(|_| shown == Shown::Image).map(|kept| {
        let image = img(PathBuf::from(kept)).max_w(px(IMAGE_W.min(width))).max_h(px(IMAGE_H)).rounded(px(6.)).object_fit(ObjectFit::Contain);
        div().p(px(6.)).pb(px(0.)).child(image)
    });
    let failed = call.status == ToolStatus::Failed;
    let line = div()
        .h(px(34.))
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(8.))
        .child(file_icon(path, false, false, 14.))
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(shared::name(path).to_string()))
        .child(div().flex_1().min_w_0().truncate().text_color(if failed { TEXT_3 } else { TEXT }).child(shared::detail(call)))
        .children((shown == Shown::App && kept.is_some()).then(|| icon("external", 12., TEXT_3)));
    let card = div()
        .id(SharedString::from(format!("{id}-shared")))
        .max_w(px(width))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(10.))
        .border_1()
        .border_color(hairline(0.08))
        .bg(ink(0.045))
        .text_size(px(TEXT_SIZE))
        .children(image)
        .child(line)
        .when(kept.is_some(), |d| d.cursor_pointer().hover(|s| s.bg(ink(0.07))).on_click(on_click));
    div().py(px(4.)).flex().child(card)
}
```

In `transcript.rs`:

1. Add `shared_card` to the `use super::{…}` list at `:8`.
2. At the top of `chat_row` (right after the `let (provider, model) = …;` line, before `let Some(view) = …`), add:

```rust
        let cwd = self.agents.get(agent).map(|a| a.cwd.clone()).unwrap_or_default();
```

3. Replace the placeholder `Row::Shared { .. } => div(),` with:

```rust
            Row::Shared { turn, block } => match t.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
                Some(Block::Tool(call)) => {
                    let path = match &call.kind {
                        ToolKind::Shared { path, .. } => path.as_str(),
                        _ => "",
                    };
                    let host = hosts::split(&cwd).0.map(str::to_string);
                    let shown = chat::shared::shown(path, host.is_some());
                    let kept = chat::shared::kept(call).map(str::to_string);
                    let target = kept.clone().map(|k| host.as_deref().map_or_else(|| k.clone(), |h| hosts::place(h, &k)));
                    let on_click = cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(target) = &target {
                            this.chat_open_shared(target, shown, cx);
                        }
                    });
                    shared_card::card(&id, call, shown, kept.as_deref(), width, on_click)
                }
                _ => div(),
            },
```

In `locate.rs`, add `use chat::shared::Shown;` to the imports and, after `chat_open_file`:

```rust
    /// Opens a file the agent shared: in the Mac's app for it, else in the preview.
    pub(super) fn chat_open_shared(&mut self, path: &str, shown: Shown, cx: &mut Context<Self>) {
        match shown {
            Shown::App => cx.open_with_system(Path::new(path)),
            Shown::Image | Shown::Preview => self.open_doc(Doc::File(path.to_string()), false, cx),
        }
    }
```

(`locate.rs` already imports `Path` and `Doc`; add them if the compiler says otherwise.)

**Step 4: Run the tests and lints**

Run: `cd packages/desktop && cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: build succeeds, clippy reports no new warnings, all tests PASS.

### Task 2.5: Capture the cards

**Files:**
- Create: `.ui-review/fixture/chat/shared-files.json`

**Context:** The capture fixture plays `chat/$FIXTURE_CHAT.json` as the chat agent's timeline (`.ui-review/fixture/fixture.ts:3`, `:136`). Capture steps are names in `packages/desktop/crates/pocket/src/capture.rs`: `chat` opens the chat, `chat-fold-open` opens the reply's fold. The image card reads a real file, so copy one to the path the fixture names first. CLAUDE.md asks for before/after captures of view changes: take "before" on `main` (the cards show as `share_file` tool rows inside the phase) and "after" on this branch.

**Step 1: Write the fixture**

```json
[
 {"id": "u1", "kind": "user", "text": "Chart Q3 revenue by month and send me the report."},
 {"id": "a1", "kind": "assistant", "text": "I'll plot it and write the report."},
 {"id": "t1", "kind": "tool", "call": {"toolUseId": "tu-t1", "name": "Bash", "detail": {"kind": "shell", "command": "python3 plot.py", "description": "Plot revenue"}, "status": "ok", "output": "saved out/chart.png", "durationMs": 1800}},
 {"id": "t2", "kind": "tool", "call": {"toolUseId": "tu-t2", "name": "mcp__plugin_anywhere_anywhere__share_file", "detail": {"kind": "other", "name": "mcp__plugin_anywhere_anywhere__share_file", "input": {"path": "out/chart.png", "caption": "Q3 revenue by month"}}, "status": "ok", "output": "/tmp/aw-fixture/0a1b2c3d4e5f-chart.png", "durationMs": 30}},
 {"id": "t3", "kind": "tool", "call": {"toolUseId": "tu-t3", "name": "mcp__plugin_anywhere_anywhere__share_file", "detail": {"kind": "other", "name": "mcp__plugin_anywhere_anywhere__share_file", "input": {"path": "out/report.pdf", "caption": "The full report"}}, "status": "ok", "output": "/tmp/aw-fixture/0a1b2c3d4e5f-report.pdf", "durationMs": 30}},
 {"id": "t4", "kind": "tool", "call": {"toolUseId": "tu-t4", "name": "mcp__plugin_anywhere_anywhere__share_file", "detail": {"kind": "other", "name": "mcp__plugin_anywhere_anywhere__share_file", "input": {"path": "out/notes.md", "caption": "How I read the numbers"}}, "status": "ok", "output": "/tmp/aw-fixture/0a1b2c3d4e5f-notes.md", "durationMs": 30}},
 {"id": "t5", "kind": "tool", "call": {"toolUseId": "tu-t5", "name": "mcp__plugin_anywhere_anywhere__share_file", "detail": {"kind": "other", "name": "mcp__plugin_anywhere_anywhere__share_file", "input": {"path": "out/raw.csv"}}, "status": "error", "output": "There's no file at out/raw.csv", "durationMs": 30}},
 {"id": "a2", "kind": "assistant", "text": "Above: the chart, the report and my notes. The raw CSV wasn't written; say if you want it."},
 {"id": "r1", "kind": "result", "ok": true, "durationMs": 42000}
]
```

**Step 2: Capture**

Run from the repo root:

```sh
mkdir -p /tmp/aw-fixture && cp docs/screenshot.png /tmp/aw-fixture/0a1b2c3d4e5f-chart.png && echo '# notes' > /tmp/aw-fixture/0a1b2c3d4e5f-notes.md && echo '%PDF' > /tmp/aw-fixture/0a1b2c3d4e5f-report.pdf
FIXTURE_CHAT=shared-files .ui-review/fixture/capture.sh /tmp/aw-shared-after chat-shared=chat chat-shared-open=chat,chat-fold-open
```

Expected: `/tmp/aw-shared-after` holds both screens. With the fold shut, they show the chart inline with "chart.png · Q3 revenue by month", a "report.pdf · The full report" card with an external-link icon, "notes.md · How I read the numbers", and "raw.csv · There's no file at out/raw.csv" in dimmed text, all above the closing answer. Compare with the same captures taken on `main`.

**Step 3: Check clicks in the real app**

With PR 1 installed (`make app`), have a chat Claude share an image, a markdown file and a PDF. Expected: clicking the image or the markdown opens it in the Explorer preview tab; clicking the PDF opens Preview.app.

---

## PR 3: The phone chat shows shared files

**Scope:** The Swift port: `ToolKind.shared`, `SharedFile` helpers, `Row.shared`, a `SharedCard` row, and opening the kept copy in the file preview. No inline images on the phone in v1.
**Depends on:** PR 1 (`fs.read` over the attachments folder) to open a card.
**Done when:** `make test` and `make typecheck-ios` pass, and on a phone paired to a Mac running PR 1, a shared image and markdown file open from their cards.

### Task 3.1: `ToolKind.shared` and the `SharedFile` helpers

**Files:**
- Create: `packages/ios/Packages/AnywhereKit/Sources/ChatModel/SharedFile.swift`
- Modify: `packages/ios/Packages/AnywhereKit/Sources/ChatModel/ChatModel.swift:87-98` (`ToolKind`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/ChatModel/Timeline.swift:239-258` (`kind`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/ChatModel/Reading.swift:190-200` (`permissionSubject`), `:224-235` (`toolLine`)
- Test: `packages/ios/Packages/AnywhereKit/Tests/ChatModelTests/SharedFileTests.swift`, `Tests/ChatModelTests/TimelineTests.swift`

**Context:** `ChatModel` mirrors the desktop `chat` crate; this is the port of `shared.rs` (Task 2.1) and the `kind` change (Task 2.2). The phone has no "open in app", so `previews` says whether its file preview can show a file (everything but the `APP` list), and a card that can't isn't tappable. `place` splits the kept path into the `fs.read` tree (its folder, which pocketd now serves, Task 1.5) and the name in it. Tests use Swift Testing (`import Testing`, `@testable import ChatModel`, `@Test("sentence")`). TimelineTests helpers: `fold(items("[…]"))`, `toolJSON(id, name, detailJSON, status)`, `onlyTool(t)`.

**Step 1: Write the failing tests**

`SharedFileTests.swift`:

```swift
import Testing

@testable import ChatModel

private func share(_ status: CallStatus, _ caption: String = "", _ output: String? = nil) -> Call {
    Call(id: "t", name: SharedFile.tool, kind: .shared(path: "out/chart.png", caption: caption), status: status, output: output)
}

@Test("only a share that went through has a kept copy")
func onlyAShareThatWentThroughHasAKeptCopy() {
    #expect(SharedFile.kept(share(.ok, "", "/h/attachments/0a1b-chart.png\n")) == "/h/attachments/0a1b-chart.png")
    #expect(SharedFile.kept(share(.running)) == nil)
    #expect(SharedFile.kept(share(.failed, "", "There's no file at out/chart.png")) == nil)
    #expect(SharedFile.kept(share(.ok, "", " ")) == nil)
}

@Test("a card reads the caption, that it is on its way, or why it failed")
func aCardReadsTheCaptionThatItIsOnItsWayOrWhyItFailed() {
    #expect(SharedFile.detail(share(.ok, "Q3  revenue\nby month", "/k")) == "Q3 revenue by month")
    #expect(SharedFile.detail(share(.running, "Q3")) == "Sharing…")
    #expect(SharedFile.detail(share(.failed, "Q3", "Files over 20 MB can't be shared\n")) == "Files over 20 MB can't be shared")
    #expect(SharedFile.detail(share(.failed, "Q3")) == "Couldn't share it")
}

@Test("the phone previews images and text but not files only an app opens")
func thePhonePreviewsImagesAndTextButNotFilesOnlyAnAppOpens() {
    #expect(SharedFile.previews("/r/Chart.PNG"))
    #expect(SharedFile.previews("/r/notes.md"))
    #expect(SharedFile.previews("/r.d/Makefile"))
    #expect(!SharedFile.previews("/r/report.PDF"))
    #expect(!SharedFile.previews("/r/clip.mov"))
}

@Test("a card names the file, and its copy is read from its folder by its name")
func aCardNamesTheFileAndItsCopyIsReadFromItsFolderByItsName() {
    #expect(SharedFile.name("/r/out/chart.png") == "chart.png")
    #expect(SharedFile.name(#"C:\r\chart.png"#) == "chart.png")
    let place = SharedFile.place("/Users/me/.coding-pocket/attachments/0a1b-chart.png")
    #expect(place.tree == "/Users/me/.coding-pocket/attachments" && place.path == "0a1b-chart.png")
}
```

Append to `TimelineTests.swift`:

```swift
@Test("a share_file call becomes a shared file")
func aShareFileCallBecomesASharedFile() {
    let detail = #"{"kind": "other", "name": "\#(SharedFile.tool)", "input": {"path": "out/chart.png", "caption": "Q3 revenue"}}"#
    let t = fold(items("[\(toolJSON("sh", SharedFile.tool, detail, "ok"))]"))
    #expect(onlyTool(t).kind == .shared(path: "out/chart.png", caption: "Q3 revenue"))
}
```

**Step 2: Run the tests to verify they fail**

Run: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter ChatModelTests`
Expected: FAIL to build: `cannot find 'SharedFile' in scope`, `type 'ToolKind' has no member 'shared'`.

**Step 3: Write the implementation**

In `ChatModel.swift`, add to `ToolKind` after `case question([Question])`:

```swift
    /// A file the agent shared through Anywhere's `share_file`: the path it named and its caption. The copy the user
    /// sees is the call's output (`SharedFile.kept`).
    case shared(path: String, caption: String)
```

`SharedFile.swift`:

```swift
import Foundation

/// Files an agent shares with the user through Anywhere's `share_file` tool (desktop `chat/src/shared.rs`).
public enum SharedFile {
    /// What Claude names `share_file`: the tool of the `anywhere` server in pocketd's `anywhere` plugin.
    public static let tool = "mcp__plugin_anywhere_anywhere__share_file"

    /// Files the file preview can't show, which only an app opens.
    private static let app: Set<String> = [
        "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "key", "pages", "numbers", "rtf", "heic", "mp4", "mov", "m4v", "webm", "mp3",
        "wav", "m4a", "aac", "zip",
    ]

    /// Whether the phone's file preview shows the file at `path`: an image, text, code, markdown or SVG.
    public static func previews(_ path: String) -> Bool {
        let name = name(path)
        guard let dot = name.lastIndex(of: ".") else { return true }
        return !app.contains(name[name.index(after: dot)...].lowercased())
    }

    /// Where Anywhere keeps the copy the user sees: what the tool answered, once the share went through.
    public static func kept(_ call: Call) -> String? {
        guard call.status == .ok, let out = call.output?.trimmingCharacters(in: .whitespacesAndNewlines), !out.isEmpty else { return nil }
        return out
    }

    /// What the card reads after the file's name: its caption, that it's on its way, or why it didn't go through.
    public static func detail(_ call: Call) -> String {
        guard case .shared(_, let caption) = call.kind else { return "" }
        switch call.status {
        case .running: return "Sharing…"
        case .failed:
            let out = call.output?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            return out.isEmpty ? "Couldn't share it" : out
        case .ok: return caption.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        }
    }

    /// The file's own name, as the card shows it.
    public static func name(_ path: String) -> String {
        path.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? path
    }

    /// The folder `fs.read` serves a kept copy from, and its name there.
    public static func place(_ kept: String) -> (tree: String, path: String) {
        guard let slash = kept.lastIndex(of: "/") else { return ("", kept) }
        return (String(kept[..<slash]), String(kept[kept.index(after: slash)...]))
    }
}
```

In `Timeline.swift` `kind`, add after the `AskUserQuestion` line:

```swift
    if name == SharedFile.tool { return .shared(path: input["path"]?.string ?? "", caption: input["caption"]?.string ?? "") }
```

In `Reading.swift`, add to `permissionSubject` before `case .question, .other:`:

```swift
    case .shared(let path, _): (.file, "Share", path)
```

and to `toolLine` before `case .question:`:

```swift
    case .shared(let path, _): return ("Share", .file(path))
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift build && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter ChatModelTests`
Expected: build succeeds; all ChatModelTests PASS. If the build names another non-exhaustive `switch` over `ToolKind`, add a `.shared` case there worded like the `.question` case beside it.

### Task 3.2: `Row.shared` and the card

**Files:**
- Modify: `packages/ios/Packages/AnywhereKit/Sources/ChatModel/Rows.swift` (`Row` `:1-50`, `plain` `:103-113`, `isWork` `:122-130`, `topGap` `:306`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/ChatModel/Session.swift:125` (same-content `switch`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/FeatureChat/RowContent.swift` (`RowContent` `:19-35`, builder `:203`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/FeatureChat/RowView.swift` (`RowActions` `:6-17`, `content` `:60`)
- Create: `packages/ios/Packages/AnywhereKit/Sources/FeatureChat/SharedCard.swift`
- Test: `packages/ios/Packages/AnywhereKit/Tests/ChatModelTests/RowsTests.swift`

**Context:** Port of Task 2.3, plus the phone's row view. `RowContent` is what a row draws, worked out from the session so a row redraws only when it changes; `SharedContent.opens` is the kept copy a tap opens, nil while it's on its way, if it failed, or for a file the preview can't show. `SharedCard` copies `QuestionMarker.swift`'s capsule (22 pt glyph tile on `Palette.fill3`, `ChatFont.small`, 34 pt tall, `Palette.fill1` fill, `Palette.hairline` border). `RowActions.openShared` defaults to doing nothing; Task 3.3 wires it. RowsTests helpers: `call(kind, status)`, `shell()`, `one(blocks)`.

**Step 1: Write the failing test**

Append to `RowsTests.swift`, beside `a question ends the phase and stands alone`:

```swift
@Test("a shared file ends the phase and stands alone")
func aSharedFileEndsThePhaseAndStandsAlone() {
    let t = one([shell(), call(.shared(path: "chart.png", caption: ""), .ok), shell()])
    #expect(Array(rows(t, pending: 0, sending: false)[2..<5]) == [.phase(turn: 0, blocks: 0..<1), .shared(turn: 0, block: 1), .phase(turn: 0, blocks: 2..<3)])
}
```

**Step 2: Run the test to verify it fails**

Run: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter ChatModelTests`
Expected: FAIL to build: `type 'Row' has no member 'shared'`.

**Step 3: Write the implementation**

In `Rows.swift`:

1. Add to `Row` after `case question(turn: Int, block: Int)`:

```swift
    /// A file the agent shared: a card that, like a question, ends the phase before it.
    case shared(turn: Int, block: Int)
```

2. In `id`, extend the case: `case .tasks(let ix, let block), .question(let ix, let block), .shared(let ix, let block), .compacted(let ix, let block), .note(let ix, let block):`
3. In `turn`, add `.shared(let t, _),` after `.question(let t, _),`.
4. In `plain`, replace the `.tool` case:

```swift
    case .tool(let c):
        if case .question = c.kind { rows.append(.question(turn: turn, block: b)) }
        if case .shared = c.kind { rows.append(.shared(turn: turn, block: b)) }
```

5. Replace `isWork`:

```swift
/// A step of a phase: thinking, a tool call or a sub-agent, but not a question or a shared file, which stand apart.
public func isWork(_ b: Block) -> Bool {
    switch b {
    case .thinking: return true
    case .tool(let c):
        if case .question = c.kind { return false }
        if case .shared = c.kind { return false }
        return true
    default: return false
    }
}
```

6. In `topGap`: `case (_, .question), (_, .shared), (_, .compacted), (_, .notice), (_, .note): return 8`

In `Session.swift:125`: `case .tasks(_, let block), .question(_, let block), .shared(_, let block), .compacted(_, let block), .note(_, let block):`

In `RowContent.swift`, add to `RowContent` after `case question(String)`:

```swift
    case shared(SharedContent)
```

and after `PromptContent`:

```swift
struct SharedContent: Hashable, Sendable {
    var name: String
    var detail: String
    var failed: Bool
    /// The kept copy a tap opens; nil while it's on its way, if it failed, or for a file the preview can't show.
    var opens: String?
}
```

and in the builder, after the `.question` case:

```swift
        case .shared(let turn, let block):
            guard let call = t.turns[turn].blocks[block].call, case .shared(let path, _) = call.kind else { return .hidden }
            let opens = SharedFile.kept(call).flatMap { SharedFile.previews(path) ? $0 : nil }
            return .shared(SharedContent(name: SharedFile.name(path), detail: SharedFile.detail(call), failed: call.status == .failed, opens: opens))
```

In `RowView.swift`, add to `RowActions` after `openLink`:

```swift
    /// Opens the kept copy of a file the agent shared.
    var openShared: (_ kept: String) -> Void = { _ in }
```

and to `content` after the `.question` case:

```swift
        case .shared(let s): SharedCard(shared: s) { if let kept = s.opens { actions.openShared(kept) } }
```

`SharedCard.swift`:

```swift
import DesignSystem
import SwiftUI

/// A file the agent shared: its name and caption; a tap opens it in the file preview when the phone can show it.
@MainActor
struct SharedCard: View {
    let shared: SharedContent
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                Image(systemName: "doc").font(.system(size: 11)).foregroundStyle(Palette.text2.color)
                    .frame(width: 22, height: 22).background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: 6))
                Text(shared.name).foregroundStyle(Palette.text2.color).lineLimit(1).layoutPriority(1)
                Text(shared.detail).foregroundStyle((shared.failed ? Palette.text3 : Palette.text).color).lineLimit(1)
                if shared.opens != nil {
                    Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Palette.text3.color)
                }
            }
            .font(ChatFont.small)
            .padding(.leading, 6)
            .padding(.trailing, 12)
            .frame(height: 34)
            .background(Palette.fill1.color, in: Capsule())
            .overlay(Capsule().strokeBorder(Palette.hairline.color))
        }
        .buttonStyle(.plain)
        .disabled(shared.opens == nil)
    }
}
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/ios && make test`
Expected: PASS. If the build names another non-exhaustive `switch` over `Row` or `RowContent`, add a `.shared` case there worded like the `.question` case beside it.

### Task 3.3: A tap opens the copy in the file preview

**Files:**
- Modify: `packages/ios/Packages/AnywhereKit/Sources/FeatureChat/TranscriptView.swift:71` (property), `:158-169` (`rowActions`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/FeatureChat/ChatView.swift:26-33` (`ChatView.body`), `:50-65` (`ChatScreen`)

**Context:** `ChatView` turns a file link into `Route.file(mac:tree:path:)`, which shows `FilePreviewScreen` reading the file with `fs.read`. A shared file's copy lives outside the agent's folder, so its route uses `SharedFile.place(kept)`: tree = the attachments folder, path = the copy's name. `ChatScreen` is also built by `FeatureChat/Gallery.swift:50` without the new closure, so `openShared` gets a default there.

**Step 1: Write the implementation**

In `TranscriptView.swift`, add after `let openLink: (MarkdownLink) -> Void`:

```swift
    let openShared: (String) -> Void
```

and in `rowActions`, after `openLink: openLink,`:

```swift
            openShared: openShared,
```

In `ChatView.swift`, in `ChatScreen`, add after `let openFile: (MarkdownLink) -> Void`:

```swift
    /// The kept copy of a file the agent shared.
    var openShared: (String) -> Void = { _ in }
```

pass it on: `TranscriptView(store: store, showing: $showing, openLink: openLink, openShared: openShared)`, and in `ChatView.body` replace the trailing-closure `ChatScreen(…) { link in … }` call with:

```swift
            ChatScreen(store: store, machine: host.name, openFile: { link in
                switch link {
                case .web: break
                case .file(let path, _):
                    guard let cwd = store.summary?.cwd else { return }
                    open(.file(mac: mac, tree: cwd, path: path))
                }
            }, openShared: { kept in
                let place = SharedFile.place(kept)
                open(.file(mac: mac, tree: place.tree, path: place.path))
            })
```

**Step 2: Build, test and typecheck**

Run: `cd packages/ios && make test && make typecheck-ios`
Expected: PASS; the typecheck reports no errors.

### Task 3.4: Check it on a phone

**Files:** none (manual check).

**Step 1:** Build and run the app on a device or simulator paired to a Mac running PR 1 (`packages/ios/README.md` says how; `make gen` first).
**Step 2:** From the phone, ask a Claude chat to share an image, a markdown file and a PDF.
Expected: three cards under the reply, visible with its fold shut. Tapping the image opens it zoomable in the file preview, tapping the markdown renders it, and the PDF card has no chevron and doesn't respond.

---

## After all three PRs

From `packages/desktop`: `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`. From `packages/pocketd`: `go test ./...`. From `packages/ios`: `make test`. All green before each PR merges.
