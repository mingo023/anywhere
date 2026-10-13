package accounts

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"
)

type Window struct {
	Percent float64 `json:"percent"`
	// ResetsAt is unix ms; 0 when the provider gave none.
	ResetsAt int64 `json:"resetsAt,omitempty"`
}

// Usage is an account's 5-hour and weekly limits, as of At (unix ms). Stale
// ones are the last known: the token has expired, or the fetch failed.
type Usage struct {
	Session *Window `json:"session,omitempty"`
	Week    *Window `json:"week,omitempty"`
	At      int64   `json:"at"`
	Stale   bool    `json:"stale,omitempty"`
}

// The usage endpoints; tests point them elsewhere.
var (
	ClaudeUsageURL = "https://api.anthropic.com/api/oauth/usage"
	CodexUsageURL  = "https://chatgpt.com/backend-api/wham/usage"
)

var client = &http.Client{Timeout: 10 * time.Second}

// Refresh fetches the usage of every signed-in account, or only id's when set.
// It reads tokens and never refreshes them: the CLI that owns a token does that.
func (s *Store) Refresh(ctx context.Context, id string) {
	var wg sync.WaitGroup
	for _, a := range s.List(func(string, string) int { return 0 }) {
		if a.Pending || !a.SignedIn || id != "" && a.ID != id {
			continue
		}
		wg.Go(func() {
			u, err := s.fetch(ctx, a.Provider, a.ID)
			s.mu.Lock()
			defer s.mu.Unlock()
			if s.usage == nil {
				s.usage = map[string]Usage{}
			}
			key := a.Provider + "/" + a.ID
			if err != nil {
				if old, ok := s.usage[key]; ok {
					old.Stale = true
					s.usage[key] = old
				}
				return
			}
			s.usage[key] = u
		})
	}
	wg.Wait()
}

func (s *Store) fetch(ctx context.Context, provider, id string) (Usage, error) {
	now := time.Now()
	if provider == "codex" {
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

func get(ctx context.Context, url string, headers map[string]string, v any) error {
	req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return err
	}
	for k, h := range headers {
		req.Header.Set(k, h)
	}
	req.Header.Set("User-Agent", "anywhere")
	res, err := client.Do(req)
	if err != nil {
		return err
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusOK {
		return fmt.Errorf("%s: %s", url, res.Status)
	}
	return json.NewDecoder(res.Body).Decode(v)
}
