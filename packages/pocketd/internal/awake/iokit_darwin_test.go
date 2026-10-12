package awake

import "testing"

func TestIOKitHoldsThenReleases(t *testing.T) {
	s := &IOKit{}
	if err := s.Hold("pocketd test"); err != nil {
		t.Fatal(err)
	}
	if err := s.Release(); err != nil {
		t.Fatal(err)
	}
}
