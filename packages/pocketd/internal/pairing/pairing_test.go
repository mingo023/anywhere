package pairing

import (
	"errors"
	"net/url"
	"testing"
	"time"

	"pocketd/internal/devices"
)

type clock struct{ t time.Time }

func (c *clock) now() time.Time          { return c.t }
func (c *clock) advance(d time.Duration) { c.t = c.t.Add(d) }
func newManager() (*Manager, *clock) {
	c := &clock{time.UnixMilli(1_700_000_000_000)}
	return New(c.now), c
}
func phone() (devices.Device, string, error) {
	return devices.Device{ID: "d1", Name: "iPhone"}, "tok", nil
}

func TestAnOfferCarriesAOneTimeCodeInThePairLink(t *testing.T) {
	m, c := newManager()
	o, _, err := m.Begin("100.77.122.82:4517", "Mac mini")
	if err != nil {
		t.Fatal(err)
	}
	if len(o.Code) != 22 || o.ExpiresAt != c.t.Add(5*time.Minute).UnixMilli() {
		t.Fatalf("%+v", o)
	}
	u, err := url.Parse(o.URL)
	q := u.Query()
	if err != nil || u.Scheme != "codingpocket" || u.Host != "pair" || q.Get("v") != "1" || q.Get("h") != "100.77.122.82:4517" || q.Get("c") != o.Code || q.Get("n") != "Mac mini" {
		t.Fatalf("%s", o.URL)
	}
}

func TestACodePairsOnceAndTellsTheOwner(t *testing.T) {
	m, _ := newManager()
	o, done, _ := m.Begin("h:1", "Mac")
	d, tok, err := m.Redeem(o.Code, phone)
	if err != nil || d.ID != "d1" || tok != "tok" {
		t.Fatalf("%+v %q %v", d, tok, err)
	}
	if r := <-done; r != (Result{DeviceID: "d1", Name: "iPhone"}) {
		t.Fatalf("%+v", r)
	}
	if _, _, err := m.Redeem(o.Code, phone); !errors.Is(err, ErrUsed) {
		t.Fatalf("second redeem: %v", err)
	}
}

func TestALateCodeIsExpiredNotInvalid(t *testing.T) {
	m, c := newManager()
	o, done, _ := m.Begin("h:1", "Mac")
	c.advance(5*time.Minute + time.Second)
	if _, _, err := m.Redeem(o.Code, phone); !errors.Is(err, ErrExpired) {
		t.Fatalf("got %v", err)
	}
	if r := <-done; r.Code != "pair_expired" {
		t.Fatalf("%+v", r)
	}
	if _, _, err := m.Redeem("AAAAAAAAAAAAAAAAAAAAAA", phone); !errors.Is(err, ErrInvalid) {
		t.Fatalf("got %v", err)
	}
}

func TestSpentCodesAreForgottenAfterTenMinutes(t *testing.T) {
	m, c := newManager()
	o, _, _ := m.Begin("h:1", "Mac")
	m.Redeem(o.Code, phone)
	c.advance(10*time.Minute + time.Second)
	if _, _, err := m.Redeem(o.Code, phone); !errors.Is(err, ErrInvalid) {
		t.Fatalf("got %v", err)
	}
}

func TestANewOfferVoidsTheOldOne(t *testing.T) {
	m, _ := newManager()
	old, oldDone, _ := m.Begin("h:1", "Mac")
	m.Begin("h:1", "Mac")
	if r := <-oldDone; r.Code != "pair_expired" {
		t.Fatalf("%+v", r)
	}
	if _, _, err := m.Redeem(old.Code, phone); !errors.Is(err, ErrExpired) {
		t.Fatalf("got %v", err)
	}
}

func TestCancelReportsWhetherTheCodeWasStillOpen(t *testing.T) {
	m, _ := newManager()
	o, _, _ := m.Begin("h:1", "Mac")
	if !m.Cancel(o.Code) || m.Cancel(o.Code) {
		t.Fatal("cancel should succeed once")
	}
	if _, _, err := m.Redeem(o.Code, phone); !errors.Is(err, ErrExpired) {
		t.Fatalf("got %v", err)
	}
}

func TestFiveWrongCodesLockPairingForAMinute(t *testing.T) {
	m, c := newManager()
	o, done, _ := m.Begin("h:1", "Mac")
	for range 4 {
		if _, _, err := m.Redeem("wrong", phone); !errors.Is(err, ErrInvalid) {
			t.Fatalf("got %v", err)
		}
	}
	if _, _, err := m.Redeem("wrong", phone); !errors.Is(err, ErrLocked) {
		t.Fatalf("5th: %v", err)
	}
	if r := <-done; r.Code != "pair_locked" {
		t.Fatalf("%+v", r)
	}
	if _, _, err := m.Begin("h:1", "Mac"); !errors.Is(err, ErrLocked) {
		t.Fatalf("begin while locked: %v", err)
	}
	if _, _, err := m.Redeem(o.Code, phone); !errors.Is(err, ErrLocked) {
		t.Fatalf("redeem while locked: %v", err)
	}
	c.advance(time.Minute)
	o, _, err := m.Begin("h:1", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := m.Redeem(o.Code, phone); err != nil {
		t.Fatalf("after the lock: %v", err)
	}
}

func TestWrongCodesCountOnlyAgainstTheOpenOffer(t *testing.T) {
	m, _ := newManager()
	o, _, _ := m.Begin("h:1", "Mac")
	for range 4 {
		m.Redeem("wrong", phone)
	}
	if _, _, err := m.Redeem(o.Code, phone); err != nil {
		t.Fatalf("right code: %v", err)
	}
	for range 4 {
		if _, _, err := m.Redeem("wrong", phone); !errors.Is(err, ErrInvalid) {
			t.Fatalf("no offer open: %v", err)
		}
	}
	o, _, err := m.Begin("h:1", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	for range 4 {
		if _, _, err := m.Redeem("wrong", phone); !errors.Is(err, ErrInvalid) {
			t.Fatalf("new offer: %v", err)
		}
	}
	if _, _, err := m.Redeem(o.Code, phone); err != nil {
		t.Fatalf("right code: %v", err)
	}
}

func TestAFailedSaveSpendsTheCode(t *testing.T) {
	m, _ := newManager()
	o, done, _ := m.Begin("h:1", "Mac")
	full := errors.New("disk full")
	if _, _, err := m.Redeem(o.Code, func() (devices.Device, string, error) { return devices.Device{}, "", full }); err != full {
		t.Fatalf("got %v", err)
	}
	if r := <-done; r.Code != "pair_failed" {
		t.Fatalf("%+v", r)
	}
	if _, _, err := m.Redeem(o.Code, phone); !errors.Is(err, ErrUsed) {
		t.Fatalf("got %v", err)
	}
}

func TestErrorsCarryTheirWireCode(t *testing.T) {
	for err, want := range map[error]string{ErrExpired: "pair_expired", ErrUsed: "pair_used", ErrInvalid: "pair_invalid", ErrLocked: "pair_locked", errors.New("x"): ""} {
		if got := Code(err); got != want {
			t.Errorf("%v: got %q, want %q", err, got, want)
		}
	}
}
