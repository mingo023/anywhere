package push

import (
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/statedb/dbtest"
)

func openAt(t *testing.T, path string) (*Store, error) {
	t.Helper()
	return Open(dbtest.Open(t, path))
}

func newStore(t *testing.T) *Store {
	t.Helper()
	s, err := openAt(t, filepath.Join(t.TempDir(), "pocketd.db"))
	if err != nil {
		t.Fatal(err)
	}
	return s
}

func handle(h string) *proto.PushHandle {
	return &proto.PushHandle{Handle: h, Secret: "c2VjcmV0", ExpiresAt: 1800000000}
}

func TestARegistrationIsKeptPrivatelyAndSurvivesARestart(t *testing.T) {
	path := filepath.Join(t.TempDir(), "pocketd.db")
	s, _ := openAt(t, path)
	now := time.UnixMilli(1000)
	reg := Registration{Key: make([]byte, 32), Env: "production", Alert: handle("a"), Start: handle("s"), Prefs: proto.PushPrefs{NeedsYou: true}, Frequent: true}
	if err := s.Register("d1", reg, now); err != nil {
		t.Fatal(err)
	}
	s.SetActivity("d1", "act", nil, false, now)
	again, err := openAt(t, path)
	r, ok := again.Get("d1")
	if err != nil || !ok || r.Alert.Handle != "a" || r.Start.Handle != "s" || !r.Prefs.NeedsYou || !r.Frequent || r.Activity.ID != "act" || r.UpdatedAt != 1000 {
		t.Fatalf("%+v %v", r, err)
	}
	reg.Alert = handle("a2")
	again.Register("d1", reg, now)
	if r, _ := again.Get("d1"); r.Activity == nil || r.Alert.Handle != "a2" {
		t.Fatalf("registering again lost the activity: %+v", r)
	}
}

func TestAnActivityGetsItsHandleLaterAndEndsByItsOwnId(t *testing.T) {
	s := newStore(t)
	s.Register("d1", Registration{Key: make([]byte, 32), Alert: handle("a")}, time.Now())
	s.SetActivity("d1", "act", nil, false, time.Now())
	s.SetActivity("d1", "act", handle("h"), false, time.Now())
	if r, _ := s.Get("d1"); r.Activity.Handle.Handle != "h" {
		t.Fatalf("%+v", r.Activity)
	}
	s.SetActivity("d1", "other", nil, true, time.Now())
	if r, _ := s.Get("d1"); r.Activity == nil {
		t.Fatal("another activity's end ended this one")
	}
	s.SetActivity("d1", "act", nil, true, time.Now())
	if r, _ := s.Get("d1"); r.Activity != nil {
		t.Fatalf("%+v", r.Activity)
	}
	if err := s.SetActivity("d2", "act", nil, false, time.Now()); err != ErrNotRegistered {
		t.Fatal(err)
	}
}

func TestAGoneHandleIsDroppedUnlessANewerOneReplacedIt(t *testing.T) {
	s := newStore(t)
	s.Register("d1", Registration{Key: make([]byte, 32), Alert: handle("new"), Start: handle("s")}, time.Now())
	s.Drop("d1", AlertHandle, "old")
	s.Drop("d1", StartHandle, "s")
	if r, _ := s.Get("d1"); r.Alert == nil || r.Start != nil {
		t.Fatalf("%+v", r)
	}
}

func TestRemovingADeviceForgetsItAndOnlyTheLast30SendsAreKept(t *testing.T) {
	path := filepath.Join(t.TempDir(), "pocketd.db")
	s, _ := openAt(t, path)
	s.Register("d1", Registration{Key: make([]byte, 32)}, time.Now())
	s.Remove("d1")
	for i := range 40 {
		s.Note(Sent{At: int64(i), Device: "d1", Kind: Done})
	}
	again, _ := openAt(t, path)
	sends := again.Sends()
	if _, ok := again.Get("d1"); ok || len(sends) != 30 || sends[0].At != 10 {
		t.Fatalf("%d sends from %d", len(sends), sends[0].At)
	}
}

func TestSendsKeepTheirOrderAndCountAcrossARestartThatAddsMore(t *testing.T) {
	path := filepath.Join(t.TempDir(), "pocketd.db")
	s, _ := openAt(t, path)
	for i := range 35 {
		s.Note(Sent{At: int64(i), Device: "d1", Kind: Done})
	}
	again, _ := openAt(t, path)
	again.Note(Sent{At: 35, Device: "d1", Kind: Done})
	third, _ := openAt(t, path)
	sends := third.Sends()
	if len(sends) != 30 || sends[0].At != 6 || sends[29].At != 35 {
		t.Fatalf("%d sends from %d to %d", len(sends), sends[0].At, sends[len(sends)-1].At)
	}
}

func TestARecordWithoutHandlesKeepsThemAbsentAfterARestart(t *testing.T) {
	path := filepath.Join(t.TempDir(), "pocketd.db")
	s, _ := openAt(t, path)
	s.Register("d1", Registration{Key: []byte{1, 2, 3}, Env: "sandbox"}, time.UnixMilli(5))
	again, _ := openAt(t, path)
	r, ok := again.Get("d1")
	if !ok || r.Alert != nil || r.Start != nil || r.Activity != nil || !reflect.DeepEqual(r.Key, []byte{1, 2, 3}) || r.Env != "sandbox" {
		t.Fatalf("%+v %v", r, ok)
	}
}
