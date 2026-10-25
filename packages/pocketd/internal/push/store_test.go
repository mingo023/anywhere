package push

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/proto"
)

func handle(h string) *proto.PushHandle {
	return &proto.PushHandle{Handle: h, Secret: "c2VjcmV0", ExpiresAt: 1800000000}
}

func TestARegistrationIsKeptPrivatelyAndSurvivesARestart(t *testing.T) {
	path := Path(t.TempDir())
	s, _ := Open(path)
	now := time.UnixMilli(1000)
	reg := Registration{Key: make([]byte, 32), Env: "production", Alert: handle("a"), Start: handle("s"), Prefs: proto.PushPrefs{NeedsYou: true}, Frequent: true}
	if err := s.Register("d1", reg, now); err != nil {
		t.Fatal(err)
	}
	if st, _ := os.Stat(path); st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
	s.SetActivity("d1", "act", nil, false, now)
	again, err := Open(path)
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
	s, _ := Open(Path(t.TempDir()))
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
	s, _ := Open(Path(t.TempDir()))
	s.Register("d1", Registration{Key: make([]byte, 32), Alert: handle("new"), Start: handle("s")}, time.Now())
	s.Drop("d1", AlertHandle, "old")
	s.Drop("d1", StartHandle, "s")
	if r, _ := s.Get("d1"); r.Alert == nil || r.Start != nil {
		t.Fatalf("%+v", r)
	}
}

func TestRemovingADeviceForgetsItAndOnlyTheLast30SendsAreKept(t *testing.T) {
	path := filepath.Join(t.TempDir(), "push.json")
	s, _ := Open(path)
	s.Register("d1", Registration{Key: make([]byte, 32)}, time.Now())
	s.Remove("d1")
	for i := range 40 {
		s.Note(Sent{At: int64(i), Device: "d1", Kind: Done})
	}
	again, _ := Open(path)
	sends := again.Sends()
	if _, ok := again.Get("d1"); ok || len(sends) != 30 || sends[0].At != 10 {
		t.Fatalf("%d sends from %d", len(sends), sends[0].At)
	}
}
