package owners

import (
	"context"
	"os"
	"strings"
	"sync"
	"testing"
	"time"
)

func store(t *testing.T) Store { return Store{Path(t.TempDir())} }

func TestAMintedTokenLooksUpItsOwner(t *testing.T) {
	s := store(t)
	o, token, err := s.Mint("Work PC")
	if err != nil {
		t.Fatal(err)
	}
	got, ok := s.Lookup(token)
	if !ok || got.ID != o.ID || got.Name != "Work PC" || got.TokenHash != "" {
		t.Fatalf("%+v %v", got, ok)
	}
	if _, ok := s.Lookup(token + "x"); ok {
		t.Error("a wrong token matched")
	}
	if _, ok := s.Lookup(""); ok {
		t.Error("no token matched")
	}
}

func TestTheFileKeepsOnlyAHashOfTheToken(t *testing.T) {
	s := store(t)
	_, token, _ := s.Mint("pc")
	raw, _ := os.ReadFile(s.Path)
	if strings.Contains(string(raw), token) {
		t.Fatal("token stored in the clear")
	}
	if st, _ := os.Stat(s.Path); st.Mode().Perm() != 0o600 {
		t.Errorf("mode %v", st.Mode())
	}
	list, _ := s.List()
	if len(list) != 1 || list[0].TokenHash != "" {
		t.Errorf("%+v", list)
	}
}

func TestATokenMintedByAnotherProcessWorksAtOnce(t *testing.T) {
	path := Path(t.TempDir())
	daemon, cli := Store{path}, Store{path}
	daemon.Lookup("warm")
	_, token, _ := cli.Mint("pc")
	if _, ok := daemon.Lookup(token); !ok {
		t.Fatal("not seen")
	}
}

func TestARevokedTokenNoLongerLooksUp(t *testing.T) {
	s := store(t)
	o, token, _ := s.Mint("a")
	keep, kept, _ := s.Mint("b")
	if _, err := s.Revoke(o.ID[:6]); err != nil {
		t.Fatal(err)
	}
	if _, ok := s.Lookup(token); ok || s.Has(o.ID) {
		t.Error("revoked token still works")
	}
	if got, ok := s.Lookup(kept); !ok || got.ID != keep.ID {
		t.Error("the other token was revoked too")
	}
	if _, err := s.Revoke(o.ID); err != ErrNotFound {
		t.Errorf("got %v", err)
	}
}

func TestWatchReportsARevoke(t *testing.T) {
	defer func(d time.Duration) { WatchEvery = d }(WatchEvery)
	WatchEvery = 10 * time.Millisecond
	s := store(t)
	o, _, _ := s.Mint("pc")
	revoked := make(chan struct{})
	go s.Watch(context.Background(), o.ID, func() { close(revoked) })
	s.Revoke(o.ID)
	select {
	case <-revoked:
	case <-time.After(time.Second):
		t.Fatal("revoke not seen")
	}
}

func TestConcurrentMintsKeepEveryToken(t *testing.T) {
	s := store(t)
	var wg sync.WaitGroup
	for range 20 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, _, err := (Store{s.Path}).Mint("pc"); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	if list, _ := s.List(); len(list) != 20 {
		t.Fatalf("kept %d of 20", len(list))
	}
}

func TestWatchOutlastsABriefUnreadableFile(t *testing.T) {
	defer func(d time.Duration) { WatchEvery = d }(WatchEvery)
	WatchEvery = 50 * time.Millisecond
	s := store(t)
	o, _, _ := s.Mint("pc")
	good, _ := os.ReadFile(s.Path)
	revoked := make(chan struct{})
	go s.Watch(t.Context(), o.ID, func() { close(revoked) })
	os.WriteFile(s.Path, []byte("{"), 0o600)
	time.Sleep(WatchEvery + WatchEvery/2)
	os.WriteFile(s.Path, good, 0o600)
	select {
	case <-revoked:
		t.Fatal("revoked by one failed read")
	case <-time.After(10 * WatchEvery):
	}
}

func TestWatchRevokesWhenTheFileStaysUnreadable(t *testing.T) {
	defer func(d time.Duration) { WatchEvery = d }(WatchEvery)
	WatchEvery = 10 * time.Millisecond
	s := store(t)
	o, _, _ := s.Mint("pc")
	revoked := make(chan struct{})
	go s.Watch(t.Context(), o.ID, func() { close(revoked) })
	os.WriteFile(s.Path, []byte("{"), 0o600)
	select {
	case <-revoked:
	case <-time.After(time.Second):
		t.Fatal("still owner")
	}
}

func TestAnUnreadableFileRevokesEveryToken(t *testing.T) {
	s := store(t)
	o, token, _ := s.Mint("pc")
	os.WriteFile(s.Path, []byte("{"), 0o600)
	if _, ok := s.Lookup(token); ok || s.Has(o.ID) {
		t.Error("a broken file still authorizes")
	}
}
