package accounts

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"pocketd/internal/providers"
)

// CodexUsageURL is ChatGPT's usage endpoint; tests point it elsewhere.
var CodexUsageURL = "https://chatgpt.com/backend-api/wham/usage"

type codex struct{}

func (codex) envKey() string { return "CODEX_HOME" }

func (codex) unset() []string {
	return []string{"OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN"}
}

func (codex) identity(s *Store, id string) (email, plan string) {
	claims := jwtClaims(s.codexAuth(id).Tokens.IDToken)
	return claims.Email, title(claims.Auth.Plan)
}

func (codex) signedIn(s *Store, id string) bool {
	return s.codexAuth(id).Tokens.AccessToken != ""
}

func (codex) usage(ctx context.Context, s *Store, id string) (Usage, error) {
	now := time.Now()
	a := s.codexAuth(id)
	if exp := jwtClaims(a.Tokens.AccessToken).Exp; exp > 0 && exp < now.Unix() {
		return Usage{}, fmt.Errorf("token expired")
	}
	var raw struct {
		Limit struct {
			Primary   *codexWindow `json:"primary_window"`
			Secondary *codexWindow `json:"secondary_window"`
		} `json:"rate_limit"`
	}
	if err := get(ctx, CodexUsageURL, map[string]string{"Authorization": "Bearer " + a.Tokens.AccessToken, "ChatGPT-Account-Id": a.Tokens.AccountID}, &raw); err != nil {
		return Usage{}, err
	}
	return Usage{Session: raw.Limit.Primary.window(), Week: raw.Limit.Secondary.window(), At: now.UnixMilli()}, nil
}

// sessions is there so a session can resume on another account.
func (codex) shared() []string {
	return []string{"config.toml", "AGENTS.md", "skills", "prompts", "hooks.json", "sessions"}
}

func (codex) history() string { return "sessions" }

func (codex) shareMore(*Store, string) error { return nil }

type codexWindow struct {
	Used    float64 `json:"used_percent"`
	ResetAt int64   `json:"reset_at"`
}

func (w *codexWindow) window() *Window {
	if w == nil {
		return nil
	}
	return &Window{Percent: w.Used, ResetsAt: w.ResetAt * 1000}
}

type codexAuth struct {
	Tokens struct {
		IDToken     string `json:"id_token"`
		AccessToken string `json:"access_token"`
		AccountID   string `json:"account_id"`
	} `json:"tokens"`
}

func (s *Store) codexAuth(id string) codexAuth {
	var a codexAuth
	raw, _ := os.ReadFile(filepath.Join(s.Dir(providers.Codex.ID, id), "auth.json"))
	json.Unmarshal(raw, &a)
	return a
}

type claims struct {
	Email string `json:"email"`
	Exp   int64  `json:"exp"`
	Auth  struct {
		Plan string `json:"chatgpt_plan_type"`
	} `json:"https://api.openai.com/auth"`
}

// jwtClaims reads a token's payload unverified: it only labels a login the CLI made.
func jwtClaims(token string) claims {
	var c claims
	parts := strings.Split(token, ".")
	if len(parts) == 3 {
		raw, _ := base64.RawURLEncoding.DecodeString(parts[1])
		json.Unmarshal(raw, &c)
	}
	return c
}
