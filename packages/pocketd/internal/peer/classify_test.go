package peer

import (
	"os"
	"os/exec"
	"testing"
)

func TestAChildOfATerminalRootIsThatTerminalsPtyPeer(t *testing.T) {
	p := Classify(os.Getpid(), map[int]string{os.Getppid(): "t1"})
	if p.Kind != PTY || p.Terminal != "t1" || p.Pid != os.Getpid() || p.Has(Drive) {
		t.Fatalf("%+v", p)
	}
}

func TestANearerTerminalRootWins(t *testing.T) {
	p := Classify(os.Getpid(), map[int]string{os.Getppid(): "outer", os.Getpid(): "inner"})
	if p.Terminal != "inner" {
		t.Fatalf("%+v", p)
	}
}

func TestAProcessOutsideEveryTerminalIsTheOwner(t *testing.T) {
	child := exec.Command("sleep", "5")
	if err := child.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { child.Process.Kill(); child.Wait() })
	p := Classify(os.Getpid(), map[int]string{child.Process.Pid: "t1"})
	if p.Kind != Owner || !p.Has(Own) {
		t.Fatalf("%+v", p)
	}
}

func TestADeadPidGetsObserveOnly(t *testing.T) {
	gone := exec.Command("true")
	if err := gone.Run(); err != nil {
		t.Fatal(err)
	}
	p := Classify(gone.Process.Pid, map[int]string{})
	if p.Kind != PTY || p.Terminal != "" || p.Has(Drive) {
		t.Fatalf("%+v", p)
	}
}

func TestAncestorsStartAtTheProcessItself(t *testing.T) {
	chain, err := Ancestors(os.Getpid())
	if err != nil || len(chain) < 2 || chain[0].Pid != os.Getpid() || chain[1].Pid != os.Getppid() {
		t.Fatalf("%+v %v", chain, err)
	}
}

func TestOnlyAPeerOutsideEveryTerminalMayAuthOverTheSocketAndOnlyWhereAPidCantProveTheOwner(t *testing.T) {
	outside := Principal{Kind: PTY, Scopes: PTYScopes}
	inside := Principal{Kind: PTY, Terminal: "t1", Scopes: PTYScopes}
	tcp := Principal{}
	cases := []struct {
		who     Principal
		pidOwns bool
		want    bool
	}{
		{tcp, true, true}, {tcp, false, true},
		{outside, true, false}, {outside, false, true},
		{inside, true, false}, {inside, false, false},
		{OwnerOf(1), false, false}, {Principal{Kind: Device}, false, false},
	}
	for _, c := range cases {
		if got := mayAuth(c.who, c.pidOwns); got != c.want {
			t.Errorf("mayAuth(%+v, %v) = %v", c.who, c.pidOwns, got)
		}
	}
}
