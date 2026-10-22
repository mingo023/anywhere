package accounts

import (
	"os"
	"testing"
	"time"
)

func used(s *Store, provider, id string, session, week float64) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.usage == nil {
		s.usage = map[string]Usage{}
	}
	s.usage[provider+"/"+id] = Usage{Session: &Window{Percent: session}, Week: &Window{Percent: week}}
}

func pick(t *testing.T, s *Store, id, prefer string) string {
	t.Helper()
	got, err := s.Pick("claude", id, prefer)
	if err != nil {
		t.Fatal(err)
	}
	return got
}

func TestANewSessionTakesTheLaunchsAccountThenTheProjectsThenTheDefault(t *testing.T) {
	s := store(t)
	signInClaude(t, s, BuiltIn, "me@home.dev")
	work := add(t, s, "claude", "me@work.dev")
	acme := add(t, s, "claude", "me@acme.dev")
	if got := pick(t, s, "", ""); got != BuiltIn {
		t.Fatalf("no choice anywhere: %q", got)
	}
	s.SetDefault("claude", work)
	if got := pick(t, s, "", ""); got != work {
		t.Fatalf("default: %q", got)
	}
	if got := pick(t, s, "", acme); got != acme {
		t.Fatalf("project: %q", got)
	}
	if got := pick(t, s, BuiltIn, acme); got != BuiltIn {
		t.Fatalf("launch: %q", got)
	}
	if _, err := s.Pick("claude", "gone", ""); err != ErrUnknown {
		t.Fatalf("unknown launch account: %v", err)
	}
	if got := pick(t, s, "", "gone"); got != work {
		t.Fatalf("a project's account removed since: %q", got)
	}
}

func TestASignedOutChoiceFallsBackToTheDefaultThenTheBuiltInLogin(t *testing.T) {
	s := store(t)
	signInClaude(t, s, BuiltIn, "me@home.dev")
	work := add(t, s, "claude", "me@work.dev")
	acme := add(t, s, "claude", "me@acme.dev")
	s.SetDefault("claude", work)
	os.Remove(s.Dir("claude", acme) + "/.credentials.json")
	if got := pick(t, s, "", acme); got != work {
		t.Fatalf("to the default: %q", got)
	}
	os.Remove(s.Dir("claude", work) + "/.credentials.json")
	if got := pick(t, s, "", acme); got != BuiltIn {
		t.Fatalf("to the built-in login: %q", got)
	}
}

func TestAChoiceAtItsLimitGivesWayToTheAccountWithTheMostLeft(t *testing.T) {
	s := store(t)
	signInClaude(t, s, BuiltIn, "me@home.dev")
	work := add(t, s, "claude", "me@work.dev")
	acme := add(t, s, "claude", "me@acme.dev")
	used(s, "claude", BuiltIn, 100, 40)
	used(s, "claude", work, 10, 70)
	used(s, "claude", acme, 30, 20)
	if got := pick(t, s, "", ""); got != acme {
		t.Fatalf("lowest peak: %q", got)
	}
	s.SetAutoSwitch(false)
	if got := pick(t, s, "", ""); got != BuiltIn {
		t.Fatalf("switching at a limit is off: %q", got)
	}
}

func TestWithEveryAccountAtItsLimitTheChoiceStands(t *testing.T) {
	s := store(t)
	signInClaude(t, s, BuiltIn, "me@home.dev")
	work := add(t, s, "claude", "me@work.dev")
	used(s, "claude", BuiltIn, 100, 40)
	used(s, "claude", work, 20, 100)
	if got := pick(t, s, "", ""); got != BuiltIn {
		t.Fatalf("%q", got)
	}
	if _, ok := s.Best("claude", BuiltIn); ok {
		t.Fatal("found an account with something left")
	}
}

func TestSwitchingAtALimitIsOnUntilTurnedOff(t *testing.T) {
	s := store(t)
	if !s.AutoSwitch() {
		t.Fatal("off by default")
	}
	s.SetAutoSwitch(false)
	if s.AutoSwitch() {
		t.Fatal("still on")
	}
	s.SetAutoSwitch(true)
	if !s.AutoSwitch() {
		t.Fatal("still off")
	}
}

func TestALimitThatHasResetSinceTheReadIsNoLimit(t *testing.T) {
	past, ahead := time.Now().Add(-time.Minute).UnixMilli(), time.Now().Add(time.Hour).UnixMilli()
	if (&Usage{Session: &Window{Percent: 100, ResetsAt: past}}).Limited() {
		t.Fatal("limited after its reset")
	}
	if !(&Usage{Session: &Window{Percent: 100, ResetsAt: ahead}}).Limited() {
		t.Fatal("not limited before its reset")
	}
}

func TestAnAccountWhoseUsageWasNeverReadComesAfterOneWithRoomLeft(t *testing.T) {
	s := store(t)
	signInClaude(t, s, BuiltIn, "me@home.dev")
	work := add(t, s, "claude", "me@work.dev")
	add(t, s, "claude", "me@acme.dev")
	used(s, "claude", work, 40, 60)
	if got, _ := s.Best("claude", BuiltIn); got != work {
		t.Fatalf("best %q", got)
	}
}
