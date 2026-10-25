package workspace

import (
	"context"
	"sync"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/proto"
)

// Accounts is list as account.list sends it: provider's alone when set, and
// none still signing in. Usage a fetch couldn't renew is the last known, as
// the desktop shows it.
func Accounts(list []accounts.Account, provider string) []proto.Account {
	out := []proto.Account{}
	for _, a := range list {
		if a.Pending || provider != "" && a.Provider != provider {
			continue
		}
		p := proto.Account{ID: a.ID, Provider: a.Provider, Name: name(a), Plan: a.Plan, Email: a.Email, SignedIn: a.SignedIn, IsDefault: a.Default}
		if u := a.Usage; u != nil {
			p.FiveHour, p.Week = usage(u.Session), usage(u.Week)
		}
		out = append(out, p)
	}
	return out
}

// name is the desktop's (accounts::name in pocket/src/accounts.rs): the
// label, else the email.
func name(a accounts.Account) string {
	for _, s := range []string{a.Label, a.Email} {
		if s != "" {
			return s
		}
	}
	return "Not signed in"
}

func usage(w *accounts.Window) *proto.Usage {
	if w == nil {
		return nil
	}
	return &proto.Usage{Pct: min(max(w.Percent, 0), 100), ResetsAt: w.ResetsAt / 1000}
}

// UsageTTL is how old usage may be before account.list fetches it again.
const UsageTTL = time.Minute

// AccountList answers account.list from the accounts store.
type AccountList struct {
	Store *accounts.Store
	// Running counts the sessions on an account.
	Running func(provider, id string) int

	mu      sync.Mutex
	checked time.Time
}

// List is provider's accounts, or every one for "". Usage older than
// UsageTTL is fetched first, bounded by ctx.
func (a *AccountList) List(ctx context.Context, provider string) []proto.Account {
	a.mu.Lock()
	stale := time.Since(a.checked) > UsageTTL
	if stale {
		a.checked = time.Now()
	}
	a.mu.Unlock()
	if stale {
		a.Store.Refresh(ctx, "")
	}
	return Accounts(a.Store.List(a.Running), provider)
}
