package peer

import (
	"context"
	"slices"
	"testing"

	"pocketd/internal/devices"
)

func TestAPrincipalRidesTheContext(t *testing.T) {
	if _, ok := From(context.Background()); ok {
		t.Fatal("a bare context has a principal")
	}
	p, ok := From(With(context.Background(), OwnerOf(7)))
	if !ok || p.Kind != Owner || p.Pid != 7 || !p.Has(Own) {
		t.Fatalf("%+v %v", p, ok)
	}
}

func TestAPairedPhoneHasItsDeviceScopes(t *testing.T) {
	p := FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes})
	if p.Kind != Device || p.Has(Own) || !slices.Equal(p.Names(), []string{"observe", "drive", "approve", "spawn"}) {
		t.Fatalf("%+v", p)
	}
}

func TestAPrincipalOfNoKindHasNoScope(t *testing.T) {
	if p := (Principal{Scopes: OwnerScopes}); p.Kind != None || p.Has(Observe) {
		t.Fatalf("%+v", p)
	}
}
