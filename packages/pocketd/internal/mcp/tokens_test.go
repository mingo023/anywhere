package mcp

import (
	"path/filepath"
	"testing"

	"pocketd/internal/orch"
)

func TestATokenSurvivesAPocketdRestart(t *testing.T) {
	path := filepath.Join(t.TempDir(), "orchestrator.db")
	store, err := orch.OpenPath(path)
	if err != nil {
		t.Fatal(err)
	}
	token, err := NewTokens(store).Issue("t1", "claude")
	if err != nil {
		t.Fatal(err)
	}
	store.Close()

	store, err = orch.OpenPath(path)
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	g, err := NewTokens(store).Resolve(token)
	if err != nil || g.Terminal != "t1" || g.Agent != "claude" {
		t.Fatalf("after a restart: %+v, %v", g, err)
	}
}

func TestAFreshTokenForATerminalRevokesItsOlderOnes(t *testing.T) {
	store, err := orch.OpenPath(filepath.Join(t.TempDir(), "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	tokens := NewTokens(store)
	old, _ := tokens.Issue("t1", "claude")
	other, _ := tokens.Issue("t2", "claude")
	fresh, err := tokens.Issue("t1", "claude")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := tokens.Resolve(old); err != ErrBadToken {
		t.Errorf("the older token of a restored terminal still works: %v", err)
	}
	if _, err := tokens.Resolve(fresh); err != nil {
		t.Errorf("the fresh token: %v", err)
	}
	if _, err := tokens.Resolve(other); err != nil {
		t.Errorf("another terminal's token: %v", err)
	}
}
