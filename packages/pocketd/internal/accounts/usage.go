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
	return loginOf(provider).usage(ctx, s, id)
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
