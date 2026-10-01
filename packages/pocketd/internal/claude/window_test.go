package claude

import "testing"

func TestWindowIsTheLongestKnownPrefixElseUnknown(t *testing.T) {
	for model, want := range map[string]int64{
		"claude-opus-5-5":           1_000_000,
		"claude-opus-5-5[1m]":       1_000_000,
		"claude-sonnet-4-5[1m]":     1_000_000,
		"claude-haiku-4-5-20251001": 200_000,
		"claude-sonnet-4-6":         200_000,
		"claude-opus-4-8":           1_000_000,
		"claude-3-5-sonnet":         0,
		"<synthetic>":               0,
		"":                          0,
	} {
		if got := Window(model); got != want {
			t.Errorf("%q: %d, want %d", model, got, want)
		}
	}
}

func TestTheHooksSuffixOrAnOverfullContextMeansTheLargeWindow(t *testing.T) {
	for _, c := range []struct {
		model, hook string
		used, want  int64
	}{
		{"claude-sonnet-4-5-20250929", "claude-sonnet-4-5[1m]", 1000, 1_000_000},
		{"claude-sonnet-4-5-20250929", "claude-sonnet-4-5", 1000, 200_000},
		{"claude-sonnet-4-5-20250929", "claude-opus-4-5[1m]", 1000, 200_000},
		{"claude-sonnet-4-5-20250929", "", 250_000, 1_000_000},
		{"claude-3-5-sonnet", "", 250_000, 0},
	} {
		if got := ContextWindow(c.model, c.hook, c.used); got != c.want {
			t.Errorf("%+v: %d", c, got)
		}
	}
}

func TestTokensSumTheInputOfAMainChainAnswer(t *testing.T) {
	const usage = `"usage":{"input_tokens":3,"cache_creation_input_tokens":200,"cache_read_input_tokens":40000,"output_tokens":900}`
	for raw, want := range map[string]int64{
		`{"type":"assistant","message":{"model":"claude-opus-5-5",` + usage + `}}`:                    40203,
		`{"type":"assistant","isSidechain":true,"message":{"model":"claude-opus-5-5",` + usage + `}}`: -1,
		`{"type":"assistant","message":{"model":"<synthetic>",` + usage + `}}`:                        -1,
		`{"type":"user","message":{` + usage + `}}`:                                                   -1,
		`{"type":"assistant","message":{"model":"claude-opus-5-5"}}`:                                  -1,
	} {
		got, ok := Tokens([]byte(raw))
		if !ok {
			got = -1
		}
		if got != want {
			t.Errorf("%s: %d, want %d", raw, got, want)
		}
	}
}
