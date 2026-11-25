package workspace

import (
	"testing"

	"pocketd/internal/accounts"
)

func TestASignedInAccountWithNoEmailIsNamedSignedIn(t *testing.T) {
	for want, a := range map[string]accounts.Account{
		"Signed in":     {SignedIn: true},
		"Not signed in": {},
		"a@x.com":       {SignedIn: true, Email: "a@x.com"},
		"Work":          {Label: "Work", Email: "a@x.com"},
	} {
		if got := name(a); got != want {
			t.Errorf("%+v: %q", a, got)
		}
	}
}
