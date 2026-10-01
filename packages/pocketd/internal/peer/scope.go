package peer

import (
	"context"
	"slices"

	"pocketd/internal/devices"
)

type Scope = devices.Scope

const (
	Observe Scope = "observe"
	Drive   Scope = "drive"
	Approve Scope = "approve"
	Spawn   Scope = "spawn"
	Own     Scope = "owner"
)

// Kind's zero value, None, has no scope, so a conn whose principal was never
// set fails closed.
type Kind uint8

const (
	None Kind = iota
	Owner
	Device
	PTY
)

var (
	OwnerScopes = []Scope{Observe, Drive, Approve, Spawn, Own}
	PTYScopes   = []Scope{Observe}
)

// Principal is who sits on the other end of a conn. Terminal is set for a
// PTY peer, Device for a paired phone.
type Principal struct {
	Kind     Kind
	Pid      int
	Terminal string
	Scopes   []Scope
}

func OwnerOf(pid int) Principal { return Principal{Kind: Owner, Pid: pid, Scopes: OwnerScopes} }

func FromDevice(d devices.Device) Principal {
	return Principal{Kind: Device, Scopes: d.Scopes}
}

func (p Principal) Has(s Scope) bool { return p.Kind != None && slices.Contains(p.Scopes, s) }

func (p Principal) Names() []string {
	names := make([]string, len(p.Scopes))
	for i, s := range p.Scopes {
		names[i] = string(s)
	}
	return names
}

type key struct{}

func With(ctx context.Context, p Principal) context.Context { return context.WithValue(ctx, key{}, p) }

func From(ctx context.Context) (Principal, bool) {
	p, ok := ctx.Value(key{}).(Principal)
	return p, ok
}
