package main

import (
	"testing"

	"pocketd/internal/owners"
)

func TestTheCLIKeepsOneOwnerTokenUntilItIsRevoked(t *testing.T) {
	home := t.TempDir()
	first, err := cliToken(home)
	if err != nil || first == "" {
		t.Fatalf("%q %v", first, err)
	}
	if again, _ := cliToken(home); again != first {
		t.Fatalf("minted %q again, had %q", again, first)
	}
	store := owners.Store{Path: owners.Path(home)}
	o, _ := store.Lookup(first)
	if _, err := store.Revoke(o.ID); err != nil {
		t.Fatal(err)
	}
	next, err := cliToken(home)
	if err != nil || next == first {
		t.Fatalf("after revoke: %q %v", next, err)
	}
	if _, ok := store.Lookup(next); !ok {
		t.Fatal("the new token isn't an owner")
	}
	if list, _ := store.List(); len(list) != 1 {
		t.Fatalf("%d owners, want 1", len(list))
	}
}
