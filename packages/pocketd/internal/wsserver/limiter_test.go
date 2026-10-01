package wsserver

import (
	"net/netip"
	"testing"
	"time"
)

func TestALockoutEndsAfterAMinute(t *testing.T) {
	var l limiter
	ip, other := netip.MustParseAddr("100.77.122.82"), netip.MustParseAddr("100.77.122.83")
	now := time.Now()
	if l.fail(ip, now) || l.fail(ip, now.Add(time.Second)) || l.locked(ip, now) {
		t.Fatal("locked before the third failure")
	}
	if !l.fail(ip, now.Add(2*time.Second)) || !l.locked(ip, now.Add(59*time.Second)) {
		t.Fatal("not locked after the third failure")
	}
	if l.locked(other, now) {
		t.Fatal("one address locked out another")
	}
	if l.locked(ip, now.Add(time.Minute)) || l.fail(ip, now.Add(time.Minute)) {
		t.Fatal("lockout outlived its minute")
	}
}
