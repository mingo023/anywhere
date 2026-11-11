package daemon

import (
	"slices"
	"strings"
	"testing"

	"pocketd/internal/proc"
	"pocketd/internal/providers"
)

func TestEveryProviderHasAKind(t *testing.T) {
	for _, p := range providers.All {
		if kindOf[p.ID] == nil {
			t.Errorf("%s has no kind", p.ID)
		}
	}
}

func TestProvider(t *testing.T) {
	for _, c := range []struct {
		argv []string
		want string
	}{
		{[]string{"claude"}, "claude"},
		{[]string{"/Users/me/.local/bin/claude", "--resume"}, "claude"},
		{[]string{"claude", "-p", "fix the tests"}, "claude"},
		{[]string{"claude", "auth", "login"}, ""},
		{[]string{"claude", "mcp", "serve"}, ""},
		{[]string{"claude", "doctor"}, ""},
		{[]string{"claude", "--version"}, ""},
		{[]string{"claude", "bg-agent"}, ""},
		{[]string{"codex"}, "codex"},
		{[]string{"/opt/homebrew/bin/codex", "resume", "--last"}, "codex"},
		{[]string{"codex", "exec", "fix the tests"}, "codex"},
		{[]string{"codex", "bg-agent"}, "codex"},
		{[]string{"codex", "app-server"}, ""},
		{[]string{"codex", "login"}, ""},
		{[]string{"codex", "a"}, ""},
		{[]string{"codex", "-V"}, ""},
		{[]string{"node", "/usr/local/bin/codex"}, ""},
		{[]string{"sh", "-c", "claude"}, ""},
		{[]string{"claude-helper"}, ""},
		{nil, ""},
		// Each as its process shows on this Mac (kern.procargs2), or as MonoCode names its binary.
		{[]string{"pi", "", "", ""}, "pi"},
		{[]string{"/opt/homebrew/bin/pi", "auth", "check"}, ""},
		{[]string{"node", "/opt/homebrew/bin/pi"}, ""},
		{[]string{"omp"}, "omp"},
		{[]string{"/tmp/opencode-probe/node_modules/.bin/opencode", "-s", "ses_e3718c6a5ffe3QQaS0lf2sqsXb"}, "opencode"},
		{[]string{"opencode.exe"}, "opencode"},
		{[]string{"opencode", "serve", "--port", "4096"}, ""},
		{[]string{"opencode", "acp"}, ""},
		{[]string{"opencode", "run", "fix it"}, ""},
		{[]string{"opencode", "auth", "login"}, ""},
		{[]string{"/Users/me/.local/bin/cursor-agent", "--resume", "c-1"}, "cursor"},
		{[]string{"cursor-agent", "acp"}, ""},
		{[]string{"cursor-agent", "login"}, ""},
		{[]string{"cursor"}, ""},
		{[]string{"grok", "--model", "grok-4.6"}, "grok"},
		{[]string{"grok", "agent", "stdio"}, ""},
		{[]string{"grok", "login"}, ""},
		{[]string{"fx"}, "fx"},
		{[]string{"fx", "acp"}, ""},
		{[]string{"agy"}, "antigravity"},
		{[]string{"antigravity"}, ""},
		{[]string{"agy_acp_server.par"}, ""},
		{hermesArgv(), "hermes"},
		{hermesArgv("chat", "-m", "gpt-6-sol"), "hermes"},
		{hermesArgv("--resume", "local-1"), "hermes"},
		{hermesArgv("acp"), ""},
		{hermesArgv("gateway", "run"), ""},
		{hermesArgv("-z", "say ok"), ""},
		{[]string{"hermes", "--tui"}, "hermes"},
		{[]string{"python3", "-I", "-c", "import this"}, ""},
		{[]string{"python3", "hermes_cli.main"}, ""},
		{[]string{"/bin/sh", "/Users/me/.local/bin/hermes"}, ""},
	} {
		if got := Provider(c.argv); got != c.want {
			t.Errorf("Provider(%q) = %q, want %q", c.argv, got, c.want)
		}
	}
}

// hermesArgv is a running hermes, as ps shows it: its launcher has python run a bootstrap script.
func hermesArgv(args ...string) []string {
	script := "import sys; sys.path.insert(0, '/Users/me/.hermes/hermes-agent'); from hermes_cli.main import main; main()"
	return append([]string{"/Users/me/.hermes/tools/python-3.14.9/bin/python3", "-I", "-c", script}, args...)
}

func TestAHermesRunByPythonReadsAsTheCommandTheUserTyped(t *testing.T) {
	got := invoked(hermesArgv("chat", "-m", "gpt-6-sol", "--yolo"))
	if strings.Join(got, " ") != "hermes chat -m gpt-6-sol --yolo" {
		t.Fatalf("got %q", got)
	}
	plain := []string{"python3", "-I", "-c", "print(1)", "chat"}
	if got := invoked(plain); !slices.Equal(got, plain) {
		t.Fatalf("got %q", got)
	}
}

func TestAPipeRunsEachAgentsChatCommand(t *testing.T) {
	for _, c := range []struct {
		inner string
		want  string
	}{
		{"/bin/cursor-agent acp --pocketd --access edits", "cursor"},
		{"/bin/grok --no-auto-update agent --no-leader --model grok-4.6 stdio --pocketd --access ask", "grok"},
		{"/bin/opencode acp --pocketd --access ask --plan", "opencode"},
		{"/bin/pi --mode rpc --model a/b --pocketd --access full", "pi"},
		{"/bin/omp --resume c-1 --mode rpc --pocketd --access full", "omp"},
		{"/bin/fx acp --model gpt-6 --pocketd --access ask", "fx"},
		{"/bin/hermes acp --pocketd --access ask", "hermes"},
		{"/bin/agy_acp_server.par --pocketd --access ask", "antigravity"},
		{"/bin/agy_acp_server.par --uid= --pocketd --access ask", "antigravity"},
		{"/bin/hermes gateway", ""},
		{"/bin/opencode serve", ""},
	} {
		argv := append([]string{"/bin/pocketd", "pipe"}, strings.Fields(c.inner)...)
		if got := pipedProvider(argv); got != c.want {
			t.Errorf("pipedProvider(%q) = %q, want %q", argv, got, c.want)
		}
		if got := Provider(argv); got != "" {
			t.Errorf("the pipe %q is itself read as %q", argv, got)
		}
	}
}

func TestAgentProcIsTheFirstMatchInPidOrder(t *testing.T) {
	npmCodex := []proc.Proc{
		{Pid: 200, Argv: []string{"node", "/usr/local/bin/codex"}},
		{Pid: 201, Argv: []string{"/usr/local/lib/node_modules/@openai/codex/vendor/aarch64-apple-darwin/codex/codex"}},
	}
	nested := []proc.Proc{
		{Pid: 300, Argv: []string{"claude"}},
		{Pid: 301, Argv: []string{"claude", "-p", "summarize"}},
	}
	for _, c := range []struct {
		procs    []proc.Proc
		provider string
		pid      int
	}{
		{nil, "", 0},
		{[]proc.Proc{{Pid: 100, Argv: []string{"sh"}}, {Pid: 101, Argv: []string{"sleep", "30"}}}, "", 0},
		{npmCodex, "codex", 201},
		{nested, "claude", 300},
		{[]proc.Proc{{Pid: 400, Argv: []string{"/bin/claude", "-p"}}, {Pid: 401, Argv: []string{"/bin/pocketd", "pipe", "/bin/claude", "-p"}}}, "claude", 401},
		{[]proc.Proc{{Pid: 500, Argv: []string{"/bin/pocketd", "pipe", "ls"}}}, "", 0},
		{[]proc.Proc{{Pid: 600, Argv: []string{"/bin/pocketd", "pipe", "/bin/codex", "app-server", "-c", "model=\"m\""}}, {Pid: 601, Argv: []string{"/bin/codex", "app-server"}}}, "codex", 600},
		{[]proc.Proc{{Pid: 700, Argv: []string{"/bin/codex", "app-server"}}}, "", 0},
		{[]proc.Proc{{Pid: 800, Argv: hermesArgv("--tui")}, {Pid: 801, Argv: []string{"node", "--expose-gc", "/Users/me/.hermes/hermes-agent/ui-tui/dist/entry.js"}}}, "hermes", 800},
		{[]proc.Proc{{Pid: 900, Argv: []string{"/bin/pocketd", "pipe", "/bin/hermes", "acp"}}, {Pid: 901, Argv: hermesArgv("acp")}}, "hermes", 900},
		{[]proc.Proc{{Pid: 950, Argv: hermesArgv("acp")}}, "", 0},
	} {
		provider, p, ok := agentProc(c.procs)
		if provider != c.provider || p.Pid != c.pid || ok != (c.pid != 0) {
			t.Errorf("agentProc(%+v) = %q, %d, %v", c.procs, provider, p.Pid, ok)
		}
	}
}
