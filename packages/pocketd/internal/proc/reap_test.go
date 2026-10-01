package proc

import (
	"slices"
	"strconv"
	"testing"
)

func dead(int) bool { return false }

func marked(pid, sid, parent int) Proc {
	return Proc{Pid: pid, Sid: sid, Env: []string{"PATH=/bin", Marker + "=" + strconv.Itoa(parent)}}
}

func TestReapTakesOnlyOrphansOfADeadPocketd(t *testing.T) {
	ps := []Proc{marked(10, 5, 99), marked(11, 5, 42), {Pid: 12, Sid: 5, Env: []string{"PATH=/bin"}}}
	if got := Reap(42, dead, ps); !slices.Equal(got, []int{10}) {
		t.Fatalf("Reap = %v, want [10]", got)
	}
}

func TestReapSparesALiveMarkerParent(t *testing.T) {
	alive := func(pid int) bool { return pid == 99 }
	if got := Reap(42, alive, []Proc{marked(10, 5, 99)}); got != nil {
		t.Fatalf("Reap = %v, want none", got)
	}
}

func TestReapSparesSessionLeaders(t *testing.T) {
	if got := Reap(42, dead, []Proc{marked(10, 10, 99)}); got != nil {
		t.Fatalf("Reap = %v, want none", got)
	}
}

func TestReapSkipsUnreadableEnv(t *testing.T) {
	if got := Reap(42, dead, []Proc{{Pid: 10, Sid: 5, Argv: []string{"node"}}}); got != nil {
		t.Fatalf("Reap = %v, want none", got)
	}
}

func TestReapReadsTheLastMarker(t *testing.T) {
	p := Proc{Pid: 10, Sid: 5, Env: []string{Marker + "=99", Marker + "=42"}}
	if got := Reap(42, dead, []Proc{p}); got != nil {
		t.Fatalf("Reap = %v, want none", got)
	}
}
