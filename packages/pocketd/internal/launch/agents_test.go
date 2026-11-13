package launch

import (
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/hub"
	"pocketd/internal/providers"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func TestEachNewAgentStartsInItsTUIWithTheFlagsItsCLITakes(t *testing.T) {
	for _, c := range []struct {
		provider, access      string
		plan                  bool
		model, effort, prompt string
		want                  string
	}{
		{"cursor", "ask", false, "", "", "", "cursor-agent"},
		{"cursor", "full", false, "gpt-6", "", "fix it", "cursor-agent --model gpt-6 --force fix it"},
		{"cursor", "ask", true, "", "", "", "cursor-agent --mode plan"},
		{"grok", "edits", false, "grok-4.6", "high", "", "grok --model grok-4.6 --reasoning-effort high"},
		{"grok", "full", false, "", "", "", "grok --always-approve"},
		{"grok", "full", true, "", "", "", "grok --permission-mode plan"},
		{"opencode", "ask", false, "opencode/big-pickle", "", "fix it", "opencode -m opencode/big-pickle --prompt fix it"},
		{"opencode", "full", false, "", "", "", "opencode --auto"},
		{"opencode", "ask", true, "", "", "", "opencode --agent plan"},
		{"pi", "ask", false, "deepseek/deepseek-v4-pro", "high", "-rf fix it", "pi --model deepseek/deepseek-v4-pro --thinking high -- -rf fix it"},
		{"pi", "full", true, "", "", "", "pi --tools read,grep,find,ls"},
		{"omp", "ask", false, "x/y", "max", "fix it", "omp --model x/y fix it"},
		{"omp", "ask", true, "", "", "", "omp --tools read,grep,glob,lsp"},
		{"fx", "auto", false, "gpt-6", "", "", "fx --model gpt-6"},
		{"hermes", "ask", false, "openai-codex:gpt-6-sol", "low", "fix it", "hermes chat -m openai-codex:gpt-6-sol --reasoning low -q fix it"},
		{"hermes", "full", false, "", "", "", "hermes chat --yolo"},
		{"antigravity", "ask", false, "", "", "", "agy"},
		{"antigravity", "full", false, "gemini-3.8-flash", "", "", "agy --model gemini-3.8-flash --dangerously-skip-permissions"},
		{"antigravity", "ask", true, "", "", "", "agy --mode plan"},
	} {
		s := spec(c.provider, c.access, c.plan)
		s.Model, s.Effort, s.Prompt = c.model, c.effort, c.prompt
		argv, f := argvOf(s, "")
		if f != nil || strings.Join(argv, " ") != c.want {
			t.Errorf("%s %s plan=%v: got %q %v, want %q", c.provider, c.access, c.plan, argv, f, c.want)
		}
	}
}

func TestEachNewAgentRunsForTheChatOnItsTransportAndCarriesItsLaunch(t *testing.T) {
	for _, c := range []struct {
		provider, access string
		plan             bool
		model, effort    string
		want             string
	}{
		{"cursor", "edits", false, "gpt-6", "", "cursor-agent acp --pocketd --access edits --model gpt-6"},
		{"grok", "auto", false, "", "", "grok --no-auto-update agent --no-leader stdio --pocketd --access auto"},
		{"grok", "full", false, "grok-4.6", "high", "grok --no-auto-update agent --no-leader --model grok-4.6 --reasoning-effort high --always-approve stdio --pocketd --access full --model grok-4.6 --effort high"},
		{"grok", "full", true, "", "", "grok --no-auto-update --permission-mode plan agent --no-leader stdio --pocketd --access full --plan"},
		{"opencode", "ask", true, "opencode/big-pickle", "", "opencode acp --pocketd --access ask --plan --model opencode/big-pickle"},
		{"pi", "ask", false, "deepseek/deepseek-v4-pro", "high", "pi --mode rpc --model deepseek/deepseek-v4-pro --thinking high --pocketd --access full --model deepseek/deepseek-v4-pro --effort high"},
		{"pi", "edits", true, "", "", "pi --mode rpc --tools read,grep,find,ls --pocketd --access full --plan"},
		{"omp", "ask", false, "x/y", "max", "omp --mode rpc --model x/y --pocketd --access full --model x/y --effort max"},
		{"fx", "settings", false, "gpt-6", "", "fx acp --model gpt-6 --pocketd --access settings --model gpt-6"},
		{"hermes", "full", false, "openai-codex:gpt-6-sol", "", "hermes acp --pocketd --access full --model openai-codex:gpt-6-sol"},
		{"fx", "ask", true, "", "", "fx acp --pocketd --access ask --plan"},
		{"hermes", "edits", true, "", "", "hermes acp --pocketd --access edits --plan"},
	} {
		s := spec(c.provider, c.access, c.plan)
		s.Model, s.Effort, s.Prompt, s.Chat = c.model, c.effort, "fix it", true
		argv, f := argvOf(s, "")
		if f != nil || strings.Join(argv, " ") != c.want {
			t.Errorf("%s %s plan=%v: got %q %v, want %q", c.provider, c.access, c.plan, argv, f, c.want)
			continue
		}
		want := state.Launch{Access: Access(s), Plan: c.plan, Model: c.model, Effort: c.effort}
		if got := state.Parse(c.provider, argv); !reflect.DeepEqual(got, want) {
			t.Errorf("%s: %q reads back as %+v, want %+v", c.provider, argv, got, want)
		}
		if run, _, _ := state.Cut(argv); slices.Contains(run, state.Carry) || len(run) == len(argv) {
			t.Errorf("%s: the agent would run %q", c.provider, run)
		}
	}
}

func TestAChatCarriesTheModelsOptionsAndRefusesOnesThatAreNoWord(t *testing.T) {
	s := spec("omp", "full", false)
	s.Chat, s.Options = true, map[string]string{"fast": "true"}
	argv, f := argvOf(s, "")
	if want := "omp --mode rpc --pocketd --access full --option fast=true"; f != nil || strings.Join(argv, " ") != want {
		t.Fatalf("got %q %v, want %q", argv, f, want)
	}
	for _, options := range []map[string]string{{"fast": "--yolo"}, {"a=b": "c"}, {"fast": ""}} {
		s.Options = options
		if _, f := argvOf(s, ""); f == nil || f.Code != "invalid_spec" {
			t.Errorf("%v: %v", options, f)
		}
	}
}

func TestATUIArgvReadsBackAsTheLaunchItsFlagsHold(t *testing.T) {
	for _, c := range []struct {
		provider string
		l        state.Launch
	}{
		{"grok", state.Launch{Access: "full", Model: "grok-4.6", Effort: "high"}},
		{"grok", state.Launch{Access: "settings", Plan: true}},
		{"opencode", state.Launch{Access: "full", Model: "opencode/big-pickle"}},
		{"opencode", state.Launch{Access: "settings", Plan: true}},
		{"pi", state.Launch{Access: "full", Plan: true, Model: "a/b", Effort: "xhigh"}},
		{"hermes", state.Launch{Access: "full", Model: "gpt-6-sol", Effort: "low"}},
		{"cursor", state.Launch{Access: "settings", Model: "gpt-6"}},
		{"fx", state.Launch{Access: "settings", Model: "gpt-6"}},
	} {
		s := spec(c.provider, c.l.Access, c.l.Plan)
		s.Model, s.Effort, s.Prompt = c.l.Model, c.l.Effort, "--yolo --auto --always-approve"
		argv, f := argvOf(s, "")
		if got := state.Parse(c.provider, argv); f != nil || !reflect.DeepEqual(got, c.l) {
			t.Errorf("%s: %q %v reads back as %+v, want %+v", c.provider, argv, f, got, c.l)
		}
	}
}

// pi and omp run every tool without asking (pi docs/security.md), so whatever
// access a launch asks for, it is judged and recorded as full.
func TestAnAgentThatNeverAsksIsJudgedAndRecordedAsFullAccess(t *testing.T) {
	for _, chat := range []bool{false, true} {
		for _, provider := range []string{"pi", "omp"} {
			for _, access := range []string{"settings", "ask", "edits", "auto", "full"} {
				s := spec(provider, access, false)
				s.Chat = chat
				if got := Access(s); got != "full" {
					t.Errorf("%s %s: judged as %q", provider, access, got)
				}
				argv, f := argvOf(s, "")
				if got := state.Parse(provider, argv); f != nil || got.Access != "full" {
					t.Errorf("%s %s chat=%v: %q %v recorded as %+v", provider, access, chat, argv, f, got)
				}
			}
		}
	}
	for _, provider := range []string{"claude", "codex", "hermes", "opencode"} {
		if got := Access(spec(provider, "ask", false)); got != "ask" {
			t.Errorf("%s: judged as %q", provider, got)
		}
	}
}

func TestAnAgentWithNoFlagToPlanPlansFirstOnlyInAChat(t *testing.T) {
	for _, provider := range []string{"fx", "hermes"} {
		if _, f := argvOf(spec(provider, "ask", true), ""); f == nil || f.Code != "access_not_allowed" || f.Message != providers.Get(provider).Name+" can't plan first in a terminal session" {
			t.Errorf("%s: got %+v", provider, f)
		}
	}
}

func TestAnEffortTheAgentDoesntHaveIsAnInvalidSpec(t *testing.T) {
	for _, c := range [][2]string{{"pi", "ultra"}, {"omp", "none"}, {"grok", "max"}, {"hermes", "high now"}} {
		s := spec(c[0], "ask", false)
		s.Effort = c[1]
		if _, f := argvOf(s, ""); f == nil || f.Code != "invalid_spec" {
			t.Errorf("%q: got %+v", c, f)
		}
		if ValidSwitch(c[0], "", c[1]) == nil {
			t.Errorf("%q switched", c)
		}
	}
	if _, f := argvOf(spec("grok", "ask", false), ""); f != nil {
		t.Fatal(f)
	}
	if err := ValidSwitch("pi", "deepseek/deepseek-v4-pro", "xhigh"); err != nil {
		t.Fatal(err)
	}
}

func TestANewAgentForksWhereItsCLIForksFromTheCommandLine(t *testing.T) {
	for _, c := range []struct{ provider, want string }{
		// An ACP agent forks over its protocol, so its command-line fork stays in the TUI.
		{"opencode", "opencode -s ses_1 --fork"},
		{"pi", "pi --fork ses_1 --mode rpc --pocketd --access full"},
	} {
		s := spec(c.provider, "ask", false)
		s.Fork, s.Chat = "ses_1", true
		argv, f := argvOf(s, "")
		if f != nil || strings.Join(argv, " ") != c.want {
			t.Errorf("%s: got %q %v, want %q", c.provider, argv, f, c.want)
		}
	}
	for _, provider := range []string{"cursor", "grok", "omp", "fx", "hermes", "antigravity"} {
		s := spec(provider, "ask", false)
		s.Fork = "ses_1"
		if _, f := argvOf(s, ""); f == nil || f.Code != "fork_unsupported" {
			t.Errorf("%s: got %+v", provider, f)
		}
	}
}

func TestANewAgentResumesInItsTUIAndForTheChat(t *testing.T) {
	for _, c := range []struct {
		provider string
		l        state.Launch
		chat     bool
		want     string
	}{
		{"cursor", state.Launch{Access: "settings", Model: "gpt-6"}, false, "cursor-agent --resume c-1 --model gpt-6"},
		{"opencode", state.Launch{Access: "full", Plan: true}, false, "opencode -s c-1 --agent plan"},
		{"opencode", state.Launch{Access: "full"}, false, "opencode -s c-1"},
		{"pi", state.Launch{Access: "full", Model: "a/b", Effort: "high"}, false, "pi --session c-1 --model a/b --thinking high"},
		{"omp", state.Launch{Access: "full"}, false, "omp --resume c-1"},
		{"hermes", state.Launch{Access: "full", Model: "gpt-6-sol"}, false, "hermes chat --resume c-1 -m gpt-6-sol"},
		{"hermes", state.Launch{Access: "full", Model: "gpt-6-sol"}, true, "hermes acp --pocketd --access ask --model gpt-6-sol"},
		{"grok", state.Launch{Access: "full", Effort: "high"}, true, "grok --no-auto-update agent --no-leader --reasoning-effort high stdio --pocketd --access ask --effort high"},
		{"pi", state.Launch{Access: "full", Plan: true, Model: "a/b"}, true, "pi --session-id c-1 --mode rpc --tools read,grep,find,ls --model a/b --pocketd --access full --plan --model a/b"},
		{"omp", state.Launch{Access: "full", Effort: "low"}, true, "omp --resume c-1 --mode rpc --pocketd --access full --effort low"},
		{"antigravity", state.Launch{Access: "edits"}, true, "agy acp --pocketd --access edits"},
	} {
		s := saved(c.provider, c.l)
		s.Chat = c.chat
		argv, f := resumeOf(s)
		if got := strings.Join(argv, " "); f != nil || got != c.want {
			t.Errorf("%s chat=%v: got %q %v, want %q", c.provider, c.chat, argv, f, c.want)
		}
	}
	// No resume flag is known for these, so a resume is refused until Settings names one.
	for _, provider := range []string{"grok", "fx", "antigravity"} {
		if _, f := resumeOf(saved(provider, state.Launch{Access: "ask"})); f == nil || f.Code != "resume_not_accepted" {
			t.Errorf("%s: got %+v", provider, f)
		}
	}
}

func TestAMoveKeepsANewAgentsFullAccess(t *testing.T) {
	s := saved("hermes", state.Launch{Access: "full"})
	s.KeepAccess = true
	if argv, _ := resumeOf(s); !slices.Contains(argv, "--yolo") {
		t.Fatalf("got %q", argv)
	}
	s.Chat = true
	if argv, _ := resumeOf(s); state.Parse("hermes", argv).Access != "full" {
		t.Fatalf("got %q", argv)
	}
}

func TestAChatResumeRunsTheAgentBehindAPipe(t *testing.T) {
	s := saved("antigravity", state.Launch{Access: "ask"})
	s.Chat = true
	var asked string
	found := func(p string) (string, bool) { asked = p; return "/opt/agy", true }
	_, args, reason := resumeCmd(s, config.DefaultArgs("antigravity"), "/bin/zsh", "/pd", nil, found)
	if reason != "" || asked != "antigravity" || !strings.Contains(strings.Join(args, " "), "/pd pipe /opt/agy acp") {
		t.Fatalf("resumeCmd = %q %q", args, reason)
	}
}

func TestSignInRunsTheCommandEachCLISignsInWith(t *testing.T) {
	for provider, want := range map[string]string{
		"claude": "x auth login", "codex": "x login --device-auth", "cursor": "x login", "grok": "x login", "opencode": "x auth login",
		"fx": "x login", "hermes": "x model", "antigravity": "x", "pi": "", "omp": "",
	} {
		if got := strings.Join(loginArgv(provider, "x"), " "); got != want {
			t.Errorf("%s: got %q, want %q", provider, got, want)
		}
	}
	for _, p := range providers.All {
		if p != &providers.Claude && p != &providers.Codex && clis[p.ID].logout("x") != nil {
			t.Errorf("%s has a logout, and no account to sign out of", p.ID)
		}
	}
}

func TestAnAgentIsFoundByItsOwnCommandOrTheOneConfigNames(t *testing.T) {
	onPath(t, "cursor-agent", "agy", "pi", "cursor", "antigravity")
	l := launcher(t, `[]`)
	env := os.Environ()
	for provider, want := range map[string]string{"cursor": "cursor-agent", "antigravity": "agy", "pi": "pi"} {
		if path, ok := l.find(provider, env); !ok || filepath.Base(path) != want {
			t.Fatalf("%s runs %q", provider, path)
		}
	}
	custom := filepath.Join(t.TempDir(), "cursor-nightly")
	os.WriteFile(custom, []byte("#!/bin/sh\n"), 0o755)
	l.set.Set("cursor.command", custom)
	if path, _ := l.find("cursor", env); path != custom {
		t.Fatalf("cursor runs %q", path)
	}
}

func TestAnAgentMissingFromOneEnvIsFoundInAnotherThatHasIt(t *testing.T) {
	onPath(t)
	l := launcher(t, `[]`)
	if _, ok := l.find("claude", os.Environ()); ok {
		t.Fatal("claude found before it was on the PATH")
	}
	onPath(t, "claude")
	if _, ok := l.find("claude", os.Environ()); !ok {
		t.Fatal("claude stayed missing in an env that has it")
	}
}

func TestACommandNamedAsAnAgentIsItOnlyWhenItsHelpSaysSo(t *testing.T) {
	onPath(t, "fx", "pi", "omp")
	env := os.Environ()
	l := launcher(t, `[]`)
	for _, provider := range []string{"fx", "pi", "omp"} {
		if _, ok := l.find(provider, env); !ok {
			t.Errorf("%s isn't found", provider)
		}
	}
	bin, asked := filepath.SplitList(os.Getenv("PATH"))[0], filepath.Join(t.TempDir(), "asked")
	for _, name := range []string{"fx", "pi", "omp"} {
		os.WriteFile(filepath.Join(bin, name), []byte("#!/bin/sh\necho $0 >> "+asked+"\necho 'Usage: "+name+" file.json, --mode tree'\n"), 0o755)
	}
	l = launcher(t, `[]`)
	for range 2 {
		for _, provider := range []string{"fx", "pi", "omp"} {
			if path, ok := l.find(provider, env); ok {
				t.Errorf("%s runs %q, which isn't it", provider, path)
			}
		}
	}
	for _, p := range l.Providers() {
		if p.Available {
			t.Errorf("%s is offered", p.ID)
		}
	}
	if b, _ := os.ReadFile(asked); strings.Count(string(b), "\n") != 3 {
		t.Fatalf("each is asked once, not:\n%s", b)
	}
}

// An agent pocketd only watches in its terminal never attaches: its create
// must not sit out attachWait for it.
func TestACreateOfAnAgentThatNeverAttachesEndsOnceItRuns(t *testing.T) {
	l := launcher(t, `[]`)
	l.d = &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New())}
	t.Cleanup(func() { l.d.Terminals.CloseAll(0) })
	term, err := l.d.Terminals.Spawn(terminal.Spec{Cmd: "/bin/sh", Args: []string{"-c", "sleep 30"}})
	if err != nil {
		t.Fatal(err)
	}
	id := term.Info().ID
	a := l.d.Agents.Add("a-1", "/w", "pi", nil)
	a.SetTerminal(id)
	a.SetAttached(false)
	for _, attaches := range []bool{false, true} {
		done := make(chan Result, 1)
		go func() { done <- l.wait(term, id, make(chan exit, 2), attaches, quiet) }()
		select {
		case r := <-done:
			if attaches || r.AgentID != "a-1" || r.Err != nil {
				t.Fatalf("attaches=%v: got %+v", attaches, r)
			}
		case <-time.After(3 * time.Second):
			if !attaches {
				t.Fatal("the create waited for an agent that never attaches")
			}
		}
	}
	a.TurnEnded(true)
	if r := l.wait(term, id, make(chan exit, 2), true, quiet); r.AgentID != "a-1" || r.Err != nil {
		t.Fatalf("a chat that failed to open: got %+v", r)
	}
	for provider, want := range map[string]bool{"claude": true, "codex": true, "pi": false, "hermes": false} {
		s := spec(provider, "ask", false)
		if got := clis[provider].attaches(s); got != want {
			t.Errorf("%s attaches in its TUI: %v", provider, got)
		}
		s.Chat = true
		if !clis[provider].attaches(s) {
			t.Errorf("%s's chat doesn't attach", provider)
		}
	}
}
