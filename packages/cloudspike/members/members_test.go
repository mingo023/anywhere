package members

import (
	"bytes"
	"crypto/ed25519"
	"encoding/json"
	"errors"
	"slices"
	"strings"
	"testing"
)

type device struct {
	key ed25519.PrivateKey
	m   Member
}

func newDevice(kind Kind, name string) device {
	pub, priv, _ := ed25519.GenerateKey(nil)
	return device{priv, Member{Key: pub, Kind: kind, Name: name}}
}

func pub(k ed25519.PrivateKey) ed25519.PublicKey { return k.Public().(ed25519.PublicKey) }

func account(t *testing.T) (Log, device, device, string) {
	t.Helper()
	mac, host := newDevice(Desktop, "mac"), newDevice(Machine, "host")
	code := NewRecoveryCode()
	l := Genesis(mac.key, mac.m, RecoveryKey(code)).Add(mac.key, host.m)
	return l, mac, host, code
}

func TestAMemberCanAddAndRemove(t *testing.T) {
	l, mac, host, _ := account(t)
	phone := newDevice(Phone, "phone")
	l = l.Add(mac.key, phone.m).Remove(host.key, mac.m.Key)
	s, err := Replay(l)
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := s.Has(mac.m.Key); ok || s.Len() != 2 {
		t.Fatalf("members = %d, mac still in = %v", s.Len(), ok)
	}
}

func TestTheCloudCantApproveADeviceWithItsOwnKey(t *testing.T) {
	l, _, _, _ := account(t)
	cloud := newDevice(Desktop, "cloud")
	if _, err := Replay(l.Add(cloud.key, cloud.m)); !errors.Is(err, ErrNotMember) {
		t.Fatalf("err = %v, want ErrNotMember", err)
	}
}

func TestTheCloudCantSwapTheKeyInAnApproval(t *testing.T) {
	l, mac, _, _ := account(t)
	phone, cloud := newDevice(Phone, "phone"), newDevice(Phone, "phone")
	l = l.Add(mac.key, phone.m)
	l[len(l)-1].Member.Key = cloud.m.Key
	if _, err := Replay(l); !errors.Is(err, ErrBadSig) {
		t.Fatalf("err = %v, want ErrBadSig", err)
	}
}

func TestTheCloudCantDropAnOpFromTheMiddle(t *testing.T) {
	l, mac, _, _ := account(t)
	phone := newDevice(Phone, "phone")
	l = l.Add(mac.key, phone.m).Remove(mac.key, phone.m.Key)
	if _, err := Replay(append(Log{l[0]}, l[2:]...)); !errors.Is(err, ErrBadChain) {
		t.Fatalf("err = %v, want ErrBadChain", err)
	}
}

func TestAMachineRejectsAnAccountTheCloudMadeUp(t *testing.T) {
	l, _, host, _ := account(t)
	v, _ := NewView(l)
	cloud := newDevice(Desktop, "cloud")
	fake := Genesis(cloud.key, cloud.m, nil).Add(cloud.key, host.m)
	if err := v.Accept(fake); !errors.Is(err, ErrOtherAccount) {
		t.Fatalf("err = %v, want ErrOtherAccount", err)
	}
}

func TestANewMachineRejectsAnApprovalFromAnAccountTheCloudMadeUp(t *testing.T) {
	l, mac, _, _ := account(t)
	real, _ := Replay(l)
	newHost, cloud := newDevice(Machine, "new"), newDevice(Desktop, "cloud")
	fake := Genesis(cloud.key, cloud.m, nil).Add(cloud.key, newHost.m)

	nd, na := NewNonce(), NewNonce()
	shownByRealMember := SAS(real.Root, newHost.m.Key, nd, na)
	if _, err := Join(fake, newHost.m.Key, nd, na, shownByRealMember); !errors.Is(err, ErrCodeMismatch) {
		t.Fatalf("err = %v, want ErrCodeMismatch", err)
	}
	if _, err := Join(l.Add(mac.key, newHost.m), newHost.m.Key, nd, na, shownByRealMember); err != nil {
		t.Fatalf("real approval: %v", err)
	}
}

// The cloud sees only Commit(nd) when it must fix its fake root and nonce,
// so it can't grind a root whose code matches the real one.
func TestTheCodeIsUnknownUntilTheNewDeviceRevealsItsNonce(t *testing.T) {
	l, _, _, _ := account(t)
	real, _ := Replay(l)
	newHost := newDevice(Machine, "new")
	na := NewNonce()
	codes := map[string]bool{}
	for range 20 {
		codes[SAS(real.Root, newHost.m.Key, NewNonce(), na)] = true
	}
	if len(codes) < 19 {
		t.Fatalf("20 nonces gave %d codes", len(codes))
	}
}

func TestTheApproverRejectsANonceTheNewDeviceDidNotCommitTo(t *testing.T) {
	nd := NewNonce()
	if err := CheckReveal(Commit(nd), nd); err != nil {
		t.Fatal(err)
	}
	if err := CheckReveal(Commit(nd), NewNonce()); !errors.Is(err, ErrCodeMismatch) {
		t.Fatalf("err = %v, want ErrCodeMismatch", err)
	}
}

func TestAPhoneCanAddAPhoneButNotADesktopOrMachine(t *testing.T) {
	l, mac, _, code := account(t)
	phone, other := newDevice(Phone, "phone"), newDevice(Phone, "other")
	l = l.Add(mac.key, phone.m)
	if _, err := Replay(l.Add(phone.key, other.m)); err != nil {
		t.Fatalf("phone adding phone: %v", err)
	}
	for _, kind := range []Kind{Desktop, Machine} {
		if _, err := Replay(l.Add(phone.key, newDevice(kind, "x").m)); !errors.Is(err, ErrScope) {
			t.Fatalf("phone adding %s: err = %v, want ErrScope", kind, err)
		}
	}
	if _, err := Replay(l.Add(RecoveryKey(code), newDevice(Desktop, "y").m)); err != nil {
		t.Fatalf("recovery adding desktop: %v", err)
	}
}

func TestTheRecoveryCodeHas130BitsAndNeverEntersTheLog(t *testing.T) {
	l, _, _, code := account(t)
	wire, _ := json.Marshal(l)
	norm := strings.ReplaceAll(code, "-", "")
	if bytes.Contains(wire, []byte(norm)) || bytes.Contains(wire, []byte(code)) {
		t.Fatal("log carries the code")
	}
	if len(norm) != 26 {
		t.Fatalf("code has %d chars, want 26 (130 bits)", len(norm))
	}
}

func TestTheRecoveryCodeAddsADeviceWithNoApproval(t *testing.T) {
	l, _, _, code := account(t)
	laptop := newDevice(Desktop, "laptop")
	l2, v, err := JoinWithCode(l, laptop.m, code)
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := v.State().Has(laptop.m.Key); !ok {
		t.Fatal("laptop not a member")
	}
	if _, err := Replay(l2); err != nil {
		t.Fatal(err)
	}
	if _, _, err := JoinWithCode(l, laptop.m, NewRecoveryCode()); !errors.Is(err, ErrCodeMismatch) {
		t.Fatalf("wrong code: err = %v", err)
	}
}

func TestTheRecoveryCodeRejectsAGenesisTheCloudMadeWithTheRealRecoveryKey(t *testing.T) {
	l, _, _, code := account(t)
	real, _ := Replay(l)
	cloud, laptop := newDevice(Desktop, "cloud"), newDevice(Desktop, "laptop")
	fake := Genesis(cloud.key, cloud.m, nil)
	fake[0].Recovery = real.Recovery
	fake[0] = sign(cloud.key, fake[0])
	if _, _, err := JoinWithCode(fake, laptop.m, code); !errors.Is(err, ErrBadSig) {
		t.Fatalf("err = %v, want ErrBadSig", err)
	}
}

func TestAMalformedLogIsRejectedNotAPanic(t *testing.T) {
	l, _, _, _ := account(t)
	short := slices.Clone(l)
	short[1].By = short[1].By[:5]
	for name, bad := range map[string]Log{
		"empty genesis":  {{Type: opGenesis}},
		"short signer":   short,
		"short recovery": {{Type: opGenesis, Recovery: []byte{1}}},
	} {
		if _, err := Replay(bad); err == nil {
			t.Fatalf("%s: accepted", name)
		}
	}
}

func TestTheRecoveryKeyCantRemoveAMember(t *testing.T) {
	l, mac, _, code := account(t)
	if _, err := Replay(l.Remove(RecoveryKey(code), mac.m.Key)); !errors.Is(err, ErrNotMember) {
		t.Fatalf("err = %v, want ErrNotMember", err)
	}
}

func TestAResetListIsAnotherAccountToAnOldMachine(t *testing.T) {
	l, _, _, _ := account(t)
	v, _ := NewView(l)
	thief := newDevice(Phone, "thief")
	if err := v.Accept(Genesis(thief.key, thief.m, nil)); !errors.Is(err, ErrOtherAccount) {
		t.Fatalf("err = %v, want ErrOtherAccount", err)
	}
	if _, ok := v.State().Has(thief.m.Key); ok {
		t.Fatal("thief is trusted")
	}
}

func TestAMachineDropsAMemberOnceItHearsOfTheRemoval(t *testing.T) {
	l, mac, _, _ := account(t)
	phone := newDevice(Phone, "phone")
	l = l.Add(mac.key, phone.m)
	v, _ := NewView(l)
	var seen []State
	v.Subscribe(func(s State) { seen = append(seen, s) })
	if err := v.Accept(l.Remove(mac.key, phone.m.Key)); err != nil {
		t.Fatal(err)
	}
	if _, ok := v.State().Has(phone.m.Key); ok || len(seen) != 1 {
		t.Fatalf("phone still trusted, %d notices", len(seen))
	}
}

func TestAStaleLogFromTheCloudDoesNotUndoARemoval(t *testing.T) {
	l, mac, _, _ := account(t)
	phone := newDevice(Phone, "phone")
	l = l.Add(mac.key, phone.m)
	v, _ := NewView(l.Remove(mac.key, phone.m.Key))
	if err := v.Accept(l); err != nil {
		t.Fatal(err)
	}
	if _, ok := v.State().Has(phone.m.Key); ok {
		t.Fatal("stale log brought the phone back")
	}
}

func TestUntilAMemberTellsItAMachineTrustsAMemberWhoseRemovalTheCloudHid(t *testing.T) {
	l, mac, _, _ := account(t)
	phone := newDevice(Phone, "phone")
	l = l.Add(mac.key, phone.m)
	v, _ := NewView(l)
	_ = l.Remove(mac.key, phone.m.Key)
	if err := v.Accept(l); err != nil {
		t.Fatal(err)
	}
	if _, ok := v.State().Has(phone.m.Key); !ok {
		t.Fatal("expected the phone to stay trusted")
	}
}

func TestTwoBranchesFromOneHeadAreAFork(t *testing.T) {
	l, mac, host, _ := account(t)
	a, b := newDevice(Phone, "a"), newDevice(Phone, "b")
	v, _ := NewView(l.Add(mac.key, a.m))
	if err := v.Accept(l.Add(host.key, b.m)); !errors.Is(err, ErrFork) {
		t.Fatalf("err = %v, want ErrFork", err)
	}
}
