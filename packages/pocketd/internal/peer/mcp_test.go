package peer

import "testing"

func TestATokenCannotCallAWebSocketOrOpsVerb(t *testing.T) {
	who := Delegated("t1")
	if who.Has(Approve) || who.Has(Drive) || who.Has(Observe) || !who.Has(Delegate) {
		t.Fatalf("scopes = %v, want only delegate", who.Names())
	}
	for verb := range Needs {
		if r := who.Check(verb, "", ""); r == nil || r.Code != "scope_denied" {
			t.Errorf("%s: %v, want scope_denied", verb, r)
		}
	}
}

func TestATokenIsRefusedEvenWithEveryScope(t *testing.T) {
	who := Delegated("t1")
	who.Scopes = OwnerScopes
	for verb := range Needs {
		if r := who.Check(verb, "", ""); r == nil || r.Code != "scope_denied" {
			t.Errorf("%s: %v, want scope_denied", verb, r)
		}
	}
}
