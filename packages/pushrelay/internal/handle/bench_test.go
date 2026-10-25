package handle

import (
	"bytes"
	"testing"
)

// Every /v1/send opens the handle, derives its secret and checks the signature before anything else.
func BenchmarkOpenAndVerifyASend(b *testing.B) {
	s, _ := NewSealer(bytes.Repeat([]byte{1}, 32), bytes.Repeat([]byte{2}, 32))
	h, _ := s.Seal(Token{Token: "00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff", Kind: Alert, Env: Production, Exp: 4102444800})
	body := bytes.Repeat([]byte("x"), 2048)
	sig := Sign(s.Secret(h), "1792832400", body)
	for b.Loop() {
		if _, err := s.Open(h); err != nil {
			b.Fatal(err)
		}
		if !Verify(s.Secret(h), "1792832400", body, sig) {
			b.Fatal("bad signature")
		}
	}
}
