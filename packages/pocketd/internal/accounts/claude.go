package accounts

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"time"

	"pocketd/internal/atomicfile"
	"pocketd/internal/providers"
)

// ClaudeUsageURL is Anthropic's usage endpoint; tests point it elsewhere.
var ClaudeUsageURL = "https://api.anthropic.com/api/oauth/usage"

type claude struct{}

func (claude) envKey() string { return "CLAUDE_CONFIG_DIR" }

func (claude) unset() []string {
	return []string{"ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN"}
}

func (claude) identity(s *Store, id string) (email, plan string) {
	path := filepath.Join(s.Dir(providers.Claude.ID, id), ".claude.json")
	if id == BuiltIn {
		path = filepath.Join(s.Home, ".claude.json")
	}
	var cfg struct {
		OAuth struct {
			Email string `json:"emailAddress"`
			Org   string `json:"organizationType"`
		} `json:"oauthAccount"`
	}
	raw, _ := os.ReadFile(path)
	json.Unmarshal(raw, &cfg)
	return cfg.OAuth.Email, title(strings.TrimPrefix(cfg.OAuth.Org, "claude_"))
}

func (claude) signedIn(s *Store, id string) bool {
	return s.claudeToken(id).AccessToken != ""
}

func (claude) usage(ctx context.Context, s *Store, id string) (Usage, error) {
	now := time.Now()
	t := s.claudeToken(id)
	if t.ExpiresAt > 0 && t.ExpiresAt < now.UnixMilli() {
		return Usage{}, fmt.Errorf("token expired")
	}
	var raw struct {
		FiveHour *claudeWindow `json:"five_hour"`
		SevenDay *claudeWindow `json:"seven_day"`
	}
	if err := get(ctx, ClaudeUsageURL, map[string]string{"Authorization": "Bearer " + t.AccessToken, "anthropic-beta": "oauth-2025-04-20"}, &raw); err != nil {
		return Usage{}, err
	}
	return Usage{Session: raw.FiveHour.window(), Week: raw.SevenDay.window(), At: now.UnixMilli()}, nil
}

// projects is there so a session can resume on another account.
func (claude) shared() []string {
	return []string{"settings.json", "CLAUDE.md", "skills", "commands", "agents", "hooks", "plugins", "rules", "output-styles", "keybindings.json", "projects"}
}

func (claude) history() string { return "projects" }

type claudeWindow struct {
	Utilization float64   `json:"utilization"`
	ResetsAt    time.Time `json:"resets_at"`
}

func (w *claudeWindow) window() *Window {
	if w == nil {
		return nil
	}
	at := int64(0)
	if !w.ResetsAt.IsZero() {
		at = w.ResetsAt.UnixMilli()
	}
	return &Window{Percent: w.Utilization, ResetsAt: at}
}

type claudeOAuth struct {
	AccessToken string `json:"accessToken"`
	ExpiresAt   int64  `json:"expiresAt"`
}

// claudeToken is Claude Code's OAuth token for account id: in the Keychain
// item named after its folder on macOS, else in .credentials.json, where
// Claude Code keeps it when it has no Keychain.
func (s *Store) claudeToken(id string) claudeOAuth {
	var raw []byte
	if s.Secret != nil {
		v, _ := s.Secret(KeychainService(s.Dir(providers.Claude.ID, id), id == BuiltIn))
		raw = []byte(v)
	}
	if len(raw) == 0 {
		raw, _ = os.ReadFile(filepath.Join(s.Dir(providers.Claude.ID, id), ".credentials.json"))
	}
	var c struct {
		OAuth claudeOAuth `json:"claudeAiOauth"`
	}
	json.Unmarshal(raw, &c)
	return c.OAuth
}

// claudeJSONShared are the keys of ~/.claude.json that hold the user's config.
// The rest names the login or caches what it entitles, which stay per account.
var claudeJSONShared = []string{"mcpServers", "hasCompletedOnboarding"}

// shareMore copies the user's config out of ~/.claude.json.
func (claude) shareMore(s *Store, id string) error {
	home, err := os.ReadFile(filepath.Join(s.Home, ".claude.json"))
	if err != nil {
		return nil
	}
	path := filepath.Join(s.Dir(providers.Claude.ID, id), ".claude.json")
	own, _ := os.ReadFile(path)
	merged, err := mergeClaudeJSON(home, own)
	if err != nil || sameJSON(merged, own) {
		return err
	}
	return atomicfile.Write(path, merged, 0o600)
}

// sameJSON compares by value, so a running claude's own key order doesn't
// make every launch rewrite a file that claude is writing to.
func sameJSON(a, b []byte) bool {
	var x, y any
	return json.Unmarshal(a, &x) == nil && json.Unmarshal(b, &y) == nil && reflect.DeepEqual(x, y)
}

// mergeClaudeJSON is an account's .claude.json with the user's config laid
// over it: per project, the user's entry wins, and the account keeps projects
// only it has been in.
func mergeClaudeJSON(home, own []byte) ([]byte, error) {
	var h, o map[string]json.RawMessage
	if err := json.Unmarshal(home, &h); err != nil {
		return nil, err
	}
	if json.Unmarshal(own, &o) != nil || o == nil {
		o = map[string]json.RawMessage{}
	}
	for _, k := range claudeJSONShared {
		if v, ok := h[k]; ok {
			o[k] = v
		}
	}
	if v, ok := h["projects"]; ok {
		var hp, op map[string]json.RawMessage
		if json.Unmarshal(v, &hp) == nil {
			if json.Unmarshal(o["projects"], &op) != nil || op == nil {
				op = map[string]json.RawMessage{}
			}
			for dir, cfg := range hp {
				op[dir] = cfg
			}
			raw, err := json.Marshal(op)
			if err != nil {
				return nil, err
			}
			o["projects"] = raw
		}
	}
	return json.MarshalIndent(o, "", "  ")
}
