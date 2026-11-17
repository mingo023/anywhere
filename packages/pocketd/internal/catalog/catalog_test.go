package catalog

import (
	"context"
	"slices"
	"strings"
	"testing"
	"time"
)

func lists(t *testing.T, read func([]byte) ([]string, []string, string, bool), out string) ([]string, string) {
	t.Helper()
	for _, line := range strings.Split(out, "\n") {
		if models, _, more, answered := read([]byte(line)); answered {
			return models, more
		}
	}
	t.Fatalf("no line of %q answers", out)
	return nil, ""
}

func TestClaudeListsWhatInitializeAnswersButItsDefault(t *testing.T) {
	out := `{"type":"system","subtype":"init"}
{"type":"control_response","response":{"subtype":"success","request_id":"m","response":{"commands":[],"models":[{"value":"default","resolvedModel":"claude-opus-5-5","displayName":"Default (recommended)","supportedEffortLevels":["low"]},{"value":"fable","resolvedModel":"claude-fable-5-1","displayName":"Fable 5.1","supportsEffort":true,"supportedEffortLevels":["low","max"]},{"value":"claude-haiku-4-5-20251001","resolvedModel":"claude-haiku-4-5-20251001","displayName":"Haiku 4.5"}]}}}`
	if got, more := lists(t, claude, out); !slices.Equal(got, []string{"fable", "claude-haiku-4-5-20251001"}) || more != "" {
		t.Fatalf("got %q, more %q", got, more)
	}
	if _, _, _, answered := claude([]byte("Error: not logged in")); answered {
		t.Fatal("an error answered")
	}
}

func TestCodexListsAPageOfModelListAndAsksForTheNext(t *testing.T) {
	out := `{"id":1,"result":{"userAgent":"codex"}}
{"id":2,"result":{"data":[{"id":"gpt-6-sol","model":"gpt-6-sol","displayName":"GPT-6-Sol","isDefault":true},{"id":"luna","model":"gpt-5.6-luna"}],"nextCursor":"c2"}}`
	got, more := lists(t, codex, out)
	if !slices.Equal(got, []string{"gpt-6-sol", "gpt-5.6-luna"}) || more != `{"id":2,"method":"model/list","params":{"cursor":"c2"}}` {
		t.Fatalf("got %q, more %q", got, more)
	}
	if got, more := lists(t, codex, `{"id":2,"result":{"data":[{"id":"o4"}],"nextCursor":null}}`); !slices.Equal(got, []string{"o4"}) || more != "" {
		t.Fatalf("the last page: got %q, more %q", got, more)
	}
	if got, more := lists(t, codex, `{"id":2,"error":{"code":-32600,"message":"Not initialized"}}`); got != nil || more != "" {
		t.Fatalf("a refusal: got %q, more %q", got, more)
	}
}

func TestListGivesTheEffortsCodexModelsSupportOnceEachAsFirstListed(t *testing.T) {
	server := `while read -r line; do
case $line in
*'"cursor":"c2"'*) echo '{"id":2,"result":{"data":[{"model":"b","hidden":false,"supportedReasoningEfforts":[{"reasoningEffort":"medium","description":"m"},{"reasoningEffort":"ultra","description":"u"}],"defaultReasoningEffort":"medium"},{"model":"c"}],"nextCursor":null}}' ;;
*model/list*) echo '{"id":2,"result":{"data":[{"model":"a","hidden":false,"supportedReasoningEfforts":[{"reasoningEffort":"low","description":"l"},{"reasoningEffort":"medium","description":"m"}],"defaultReasoningEffort":"low"}],"nextCursor":"c2"}}' ;;
esac
done`
	models, efforts, err := List(context.Background(), "codex", []string{"sh", "-c", server}, nil)
	if err != nil || !slices.Equal(models, []string{"a", "b", "c"}) || !slices.Equal(efforts, []string{"low", "medium", "ultra"}) {
		t.Fatalf("got %q, %q, %v", models, efforts, err)
	}
}

func TestPiListsProviderSlashIDOfGetAvailableModels(t *testing.T) {
	out := `{"type":"response","command":"get_state","success":true,"data":{"model":{"id":"deepseek-v4-pro","provider":"deepseek"}}}
{"type":"response","command":"get_available_models","success":true,"data":{"models":[{"id":"amazon.nova-lite-v1:0","name":"Nova Lite","provider":"amazon-bedrock","reasoning":false},{"id":"deepseek-v4-pro","name":"DeepSeek V4 Pro","provider":"deepseek","reasoning":true},{"name":"No id","provider":"deepseek"}]}}`
	if got, _ := lists(t, pi, out); !slices.Equal(got, []string{"amazon-bedrock/amazon.nova-lite-v1:0", "deepseek/deepseek-v4-pro"}) {
		t.Fatalf("got %q", got)
	}
	if got, _ := lists(t, pi, `{"type":"response","command":"get_available_models","success":false,"error":"No models configured"}`); got != nil {
		t.Fatalf("a refusal: got %q", got)
	}
}

func TestListFollowsEveryPageThenStopsAServerThatRunsOn(t *testing.T) {
	server := `while read -r line; do
case $line in
*'"cursor":"c2"'*) echo '{"id":2,"result":{"data":[{"model":"b"}],"nextCursor":null}}' ;;
*model/list*) echo '{"id":2,"result":{"data":[{"model":"a"}],"nextCursor":"c2"}}' ;;
esac
done`
	start := time.Now()
	got, _, err := List(context.Background(), "codex", []string{"sh", "-c", server}, nil)
	if err != nil || !slices.Equal(got, []string{"a", "b"}) {
		t.Fatalf("got %q, %v", got, err)
	}
	if time.Since(start) > 5*time.Second {
		t.Fatalf("took %s", time.Since(start))
	}
}

func TestListSaysSoWhenTheCLIExitsUnanswered(t *testing.T) {
	if got, _, err := List(context.Background(), "pi", []string{"sh", "-c", "echo not logged in"}, nil); got != nil || err == nil {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestOnlyClaudeHasABackupList(t *testing.T) {
	for _, kind := range []string{"claude", "codex", "pi", "omp", "hermes"} {
		if got := Backup(kind); (kind == "claude") != slices.Equal(got, []string{"haiku", "sonnet", "opus"}) || (kind != "claude" && got != nil) {
			t.Fatalf("%s: %q", kind, got)
		}
	}
}
