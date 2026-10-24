package handle

import (
	"bytes"
	"testing"
)

func sealer(t *testing.T) *Sealer {
	s, err := NewSealer(bytes.Repeat([]byte{1}, 32), bytes.Repeat([]byte{2}, 32))
	if err != nil {
		t.Fatal(err)
	}
	return s
}

func TestASealedHandleOpensToWhatItSealed(t *testing.T) {
	s := sealer(t)
	want := Token{Token: "abcd", Kind: Activity, Env: Production, Exp: 1800000000}
	h, err := s.Seal(want)
	if err != nil {
		t.Fatal(err)
	}
	got, err := s.Open(h)
	want.V = 1
	if err != nil || got != want {
		t.Fatalf("%+v %v", got, err)
	}
}

func TestAHandleTamperedWithOrSealedUnderOtherKeysDoesNotOpen(t *testing.T) {
	s := sealer(t)
	h, _ := s.Seal(Token{Token: "abcd", Kind: Alert, Env: Production, Exp: 1})
	other, _ := NewSealer(bytes.Repeat([]byte{3}, 32), bytes.Repeat([]byte{2}, 32))
	flipped := []byte(h)
	flipped[len(flipped)/2] ^= 1
	for _, bad := range []string{"", "!!", "AAAA", string(flipped)} {
		if _, err := s.Open(bad); err != ErrInvalid {
			t.Errorf("%q opened", bad)
		}
	}
	if _, err := other.Open(h); err != ErrInvalid {
		t.Error("opened under another seal key")
	}
}

func TestEachHandleHasItsOwnSecretAndOnlyItSignsForIt(t *testing.T) {
	s := sealer(t)
	a, _ := s.Seal(Token{Token: "a", Kind: Alert, Env: Production, Exp: 1})
	b, _ := s.Seal(Token{Token: "a", Kind: Alert, Env: Production, Exp: 1})
	if bytes.Equal(s.Secret(a), s.Secret(b)) {
		t.Fatal("two handles share a secret")
	}
	sig := Sign(s.Secret(a), "100", []byte(`{}`))
	if !Verify(s.Secret(a), "100", []byte(`{}`), sig) {
		t.Fatal("own signature refused")
	}
	for _, c := range []struct {
		secret []byte
		ts     string
		body   string
		sig    string
	}{
		{s.Secret(b), "100", `{}`, sig},
		{s.Secret(a), "101", `{}`, sig},
		{s.Secret(a), "100", `{ }`, sig},
		{s.Secret(a), "100", `{}`, "not base64!"},
	} {
		if Verify(c.secret, c.ts, []byte(c.body), c.sig) {
			t.Errorf("%+v verified", c)
		}
	}
}

func TestKeysMustBe32Bytes(t *testing.T) {
	if _, err := NewSealer(make([]byte, 16), make([]byte, 32)); err != ErrKey {
		t.Fatal(err)
	}
}
