package relay

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"encoding/hex"
	"errors"
	"io"
	"net"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"cloudspike/channel"
	"cloudspike/members"
)

type device struct {
	key ed25519.PrivateKey
	m   members.Member
}

func newDevice(kind members.Kind) device {
	pub, priv, _ := ed25519.GenerateKey(nil)
	return device{priv, members.Member{Key: pub, Kind: kind}}
}

func start(t *testing.T) (*Relay, string) {
	t.Helper()
	r := New()
	srv := httptest.NewServer(r)
	t.Cleanup(srv.Close)
	return r, "ws" + strings.TrimPrefix(srv.URL, "http")
}

func serveEcho(t *testing.T, ctx context.Context, base string, host device, v *members.View) {
	t.Helper()
	id := hex.EncodeToString(host.m.Key)
	go Serve(ctx, base, "acct", id, func(raw net.Conn) {
		c, err := channel.Server(raw, host.key, v)
		if err != nil {
			raw.Close()
			return
		}
		defer c.Close()
		io.Copy(c, c)
	})
	waitOnline(t, base, id)
}

func waitOnline(t *testing.T, base, host string) {
	t.Helper()
	for range 100 {
		c, err := Dial(context.Background(), base, "acct", host)
		if err == nil {
			c.Close()
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatal("host never came online")
}

func TestAMemberReachesAMachineThroughTheRelayWhichSeesOnlyCiphertext(t *testing.T) {
	r, base := start(t)
	var mu sync.Mutex
	var seen bytes.Buffer
	r.tap = func(p []byte) {
		mu.Lock()
		seen.Write(p)
		mu.Unlock()
	}
	mac, host := newDevice(members.Desktop), newDevice(members.Machine)
	l := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m)
	hv, _ := members.NewView(l)
	cv, _ := members.NewView(l)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	serveEcho(t, ctx, base, host, hv)

	raw, err := Dial(ctx, base, "acct", hex.EncodeToString(host.m.Key))
	if err != nil {
		t.Fatal(err)
	}
	c, err := channel.Client(raw, mac.key, cv, host.m.Key)
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	secret := []byte(`{"type":"exec","cmd":"cat ~/.ssh/id_ed25519"}`)
	c.Write(secret)
	got := make([]byte, len(secret))
	if _, err := io.ReadFull(c, got); err != nil || !bytes.Equal(got, secret) {
		t.Fatalf("echo = %q, %v", got, err)
	}
	mu.Lock()
	defer mu.Unlock()
	if bytes.Contains(seen.Bytes(), []byte("id_ed25519")) || seen.Len() == 0 {
		t.Fatalf("relay saw %d bytes, plaintext in them = %v", seen.Len(), bytes.Contains(seen.Bytes(), []byte("id_ed25519")))
	}
	if r.Sent("acct") < int64(2*len(secret)) {
		t.Fatalf("sent = %d", r.Sent("acct"))
	}
}

func TestDiallingAMachineThatIsNotOnTheRelayFails(t *testing.T) {
	_, base := start(t)
	if _, err := Dial(context.Background(), base, "acct", "nobody"); !errors.Is(err, ErrOffline) {
		t.Fatalf("err = %v, want ErrOffline", err)
	}
}

func TestARemovedMemberCantReachTheMachineThroughTheRelay(t *testing.T) {
	_, base := start(t)
	mac, host, phone := newDevice(members.Desktop), newDevice(members.Machine), newDevice(members.Phone)
	l := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m).Add(mac.key, phone.m)
	hv, _ := members.NewView(l.Remove(mac.key, phone.m.Key))
	pv, _ := members.NewView(l)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	serveEcho(t, ctx, base, host, hv)

	raw, err := Dial(ctx, base, "acct", hex.EncodeToString(host.m.Key))
	if err != nil {
		t.Fatal(err)
	}
	if c, err := channel.Client(raw, phone.key, pv, host.m.Key); err == nil {
		c.SetReadDeadline(time.Now().Add(2 * time.Second))
		if _, err := c.Read(make([]byte, 1)); err == nil {
			t.Fatal("removed phone got a working channel")
		}
	}
}
