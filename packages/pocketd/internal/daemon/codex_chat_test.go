package daemon

import (
	"encoding/json"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

func TestSlashCommandSplitsANameFromItsArgButNotAPath(t *testing.T) {
	cases := []struct {
		text, name, arg string
		ok              bool
	}{
		{"/goal", "goal", "", true},
		{"/goal  tests pass ", "goal", "tests pass", true},
		{"/goal pause", "goal", "pause", true},
		{"/review-pr 12", "review-pr", "12", true},
		{"/tmp/x is gone", "", "", false},
		{"/ goal", "", "", false},
		{"goal", "", "", false},
		{"/goal\nmore", "goal", "more", true},
		{"/etc.conf", "", "", false},
	}
	for _, c := range cases {
		name, arg, ok := slashCommand(c.text)
		if name != c.name || arg != c.arg || ok != c.ok {
			t.Errorf("%q: got %q %q %v", c.text, name, arg, ok)
		}
	}
}

func TestAnAccessSwitchSetsWhatItsLaunchConfigSets(t *testing.T) {
	for access, want := range map[string][3]string{
		"ask":   {"readOnly", "on-request", "user"},
		"edits": {"workspaceWrite", "on-request", "user"},
		"auto":  {"workspaceWrite", "on-request", "auto_review"},
		"full":  {"dangerFullAccess", "never", "user"},
	} {
		s := accessSettings(access)
		if got := [3]string{s["sandboxPolicy"].(map[string]any)["type"].(string), s["approvalPolicy"].(string), s["approvalsReviewer"].(string)}; got != want {
			t.Errorf("%s: got %v", access, got)
		}
	}
}

func TestCodexMcpStatusReadsInTheWiresTermsWithItsNextPage(t *testing.T) {
	got, next := codexMcp(json.RawMessage(`{"data":[
		{"name":"fff","runtimeStatus":"connected","authStatus":"unsupported"},
		{"name":"atlassian","runtimeStatus":"authenticationRequired","authStatus":"notLoggedIn"},
		{"name":"code-review","runtimeStatus":"disabled","pluginId":"code-review@openai-bundled"},
		{"name":"slow","runtimeStatus":"starting"},
		{"name":"stopped","runtimeStatus":"cancelled"},
		{"name":"cold","runtimeStatus":null,"authStatus":"notLoggedIn"},
		{"name":"idle","runtimeStatus":null,"authStatus":"oAuth"}],"nextCursor":"c2"}`))
	want := []proto.McpServer{
		{Name: "fff", Status: "connected"},
		{Name: "atlassian", Status: "needsAuth"},
		{Name: "code-review", Status: "disabled", Scope: "plugin"},
		{Name: "slow", Status: "pending"},
		{Name: "stopped", Status: "failed"},
		{Name: "cold", Status: "needsAuth"},
		{Name: "idle", Status: "pending"},
	}
	if !reflect.DeepEqual(got, want) || next != "c2" {
		t.Fatalf("%+v %q", got, next)
	}
}
