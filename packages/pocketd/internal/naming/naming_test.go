package naming

import (
	"context"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestTheLastObjectWithNamesWins(t *testing.T) {
	out := "Sure!\n{\"title\":\"old\"}\n```json\n{\"title\":\"\\\"Fix the login bug.\\\"\",\"branchName\":\"Fix/Login Bug!!\",\"vague\":false}\n```"
	n, ok := Parse(out)
	if !ok || n != (Result{Title: "Fix the login bug", Branch: "fix-login-bug"}) {
		t.Fatalf("%+v %v", n, ok)
	}
	if _, ok := Parse("I can't name that"); ok {
		t.Fatal("parsed plain text")
	}
}

func TestBranchNamesAreShortKebabCaseAndTitlesKeepTheirLanguage(t *testing.T) {
	n, _ := Parse(`{"title":"Sửa lỗi đăng nhập!","branchName":"Add OAuth2 support to the settings page"}`)
	if n.Title != "Sửa lỗi đăng nhập" || n.Branch != "add-oauth2-support-to-the" {
		t.Fatalf("%+v", n)
	}
}

func TestThePromptFencesTheRequestAndAClippedReply(t *testing.T) {
	p := Prompt("do x", strings.Repeat("a", 2000))
	if !strings.Contains(p, "<user-prompt>\ndo x\n</user-prompt>") || !strings.Contains(p, "<agent-reply>\n"+strings.Repeat("a", MaxReply)+"\n</agent-reply>") {
		t.Fatal(p)
	}
	if strings.Contains(Prompt("do x", ""), "<agent-reply>") {
		t.Fatal("empty reply sent")
	}
	if !strings.Contains(Prompt(strings.Repeat("b", 9000), ""), "<user-prompt>\n"+strings.Repeat("b", MaxPrompt)+"\n</user-prompt>") {
		t.Fatal("long prompt not clipped")
	}
}

func TestEachProviderRunsHeadlessOnACheapModel(t *testing.T) {
	if got := Command("claude", "/bin/claude", "P"); !reflect.DeepEqual(got, []string{"/bin/claude", "--strict-mcp-config", "-p", "--model", "haiku", "P"}) {
		t.Fatal(got)
	}
	if got := Command("codex", "/bin/codex", "P"); !reflect.DeepEqual(got, []string{"/bin/codex", "exec", "--skip-git-repo-check", "-m", "gpt-5.6-luna", "P"}) {
		t.Fatal(got)
	}
}

func TestTheNamingRunNeverSeesAPIKeysOrPocketdHooks(t *testing.T) {
	got := Env([]string{"PATH=/bin", "ANTHROPIC_API_KEY=k", "OPENAI_API_KEY=k", "CLAUDECODE=1", "CLAUDE_CODE_CHILD_SESSION=1", "POCKETD_SOCK=s", "POCKETD_PTY=t", "CLAUDE_CODE_PLUGIN_DIRS=d", "HOME=/h"})
	if !reflect.DeepEqual(got, []string{"PATH=/bin", "HOME=/h"}) {
		t.Fatal(got)
	}
}

func TestGenerateParsesWhatTheCommandPrints(t *testing.T) {
	n, err := Generate(context.Background(), []string{"/bin/sh", "-c", `echo '{"title":"Say hi","branchName":"say-hi","vague":true}'`}, nil)
	if err != nil || n != (Result{Title: "Say hi", Branch: "say-hi", Vague: true}) {
		t.Fatalf("%+v %v", n, err)
	}
}

func TestGenerateGivesUpAtTheTimeout(t *testing.T) {
	old := Timeout
	Timeout = 100 * time.Millisecond
	t.Cleanup(func() { Timeout = old })
	start := time.Now()
	if _, err := Generate(context.Background(), []string{"/bin/sh", "-c", "sleep 5"}, nil); err == nil || time.Since(start) > 2*time.Second {
		t.Fatalf("%v after %v", err, time.Since(start))
	}
}
