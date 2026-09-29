package proc

import (
	"encoding/binary"
	"os"
	"os/exec"
	"slices"
	"syscall"
	"testing"
	"time"

	"github.com/creack/pty"
)

// TestHelperProcess is a child that is not an Apple binary: the kernel
// hides the env of those.
func TestHelperProcess(t *testing.T) {
	if os.Getenv("POCKET_TEST") == "" {
		return
	}
	time.Sleep(10 * time.Second)
}

func start(t *testing.T, name string, args ...string) *exec.Cmd {
	t.Helper()
	cmd := exec.Command(name, args...)
	cmd.Env = []string{"PATH=/bin:/usr/bin", "POCKET_TEST=1"}
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
		cmd.Wait()
	})
	return cmd
}

func eventually(t *testing.T, what string, ok func() bool) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(20 * time.Millisecond) {
		if ok() {
			return
		}
	}
	t.Fatalf("timed out waiting for %s", what)
}

func TestForegroundIsThePTYChild(t *testing.T) {
	cmd := exec.Command("sleep", "10")
	f, err := pty.Start(cmd)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		cmd.Process.Kill()
		cmd.Wait()
		f.Close()
	})
	if pgid, err := Foreground(f); err != nil || pgid != cmd.Process.Pid {
		t.Fatalf("Foreground = %d, %v; want %d", pgid, err, cmd.Process.Pid)
	}
}

func TestMembersListsTheGroupInPidOrder(t *testing.T) {
	pgid := start(t, "sh", "-c", "sleep 10 & sleep 10 & wait").Process.Pid
	var pids []int
	eventually(t, "three members", func() bool {
		pids, _ = Members(pgid)
		return len(pids) == 3
	})
	if !slices.IsSorted(pids) || !slices.Contains(pids, pgid) {
		t.Fatalf("members = %v", pids)
	}
}

func TestReadGivesArgvEnvAndParent(t *testing.T) {
	pid := start(t, os.Args[0], "-test.run=^TestHelperProcess$").Process.Pid
	var p Proc
	eventually(t, "env", func() bool {
		p, _ = Read(pid)
		return slices.Equal(p.Env, []string{"PATH=/bin:/usr/bin", "POCKET_TEST=1"})
	})
	if !slices.Equal(p.Argv, []string{os.Args[0], "-test.run=^TestHelperProcess$"}) {
		t.Fatalf("argv = %q", p.Argv)
	}
	if ppid, err := Parent(pid); err != nil || ppid != os.Getpid() {
		t.Fatalf("Parent = %d, %v", ppid, err)
	}
}

func TestReadOfAZombieFallsBackToTheKernelName(t *testing.T) {
	pid := start(t, "sleep", "0").Process.Pid
	var p Proc
	eventually(t, "a zombie", func() bool {
		p, _ = Read(pid)
		return len(p.Argv) == 1
	})
	if p.Argv[0] != "sleep" {
		t.Fatalf("argv = %q", p.Argv)
	}
}

func TestReadOfAGoneProcessFails(t *testing.T) {
	cmd := exec.Command("true")
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	if p, err := Read(cmd.Process.Pid); err == nil {
		t.Fatalf("Read = %+v", p)
	}
	if _, err := Parent(cmd.Process.Pid); err == nil {
		t.Fatal("Parent of a gone process")
	}
}

func procargs2(argc uint32, s string) []byte {
	return append(binary.NativeEndian.AppendUint32(nil, argc), s...)
}

func TestParse(t *testing.T) {
	for _, tc := range []struct {
		name      string
		b         []byte
		argv, env []string
	}{
		{"normal", procargs2(2, "/bin/claude\x00\x00\x00claude\x00-p\x00A=1\x00B=2\x00\x00ptr_munge=\x00"), []string{"claude", "-p"}, []string{"A=1", "B=2"}},
		{"title rewritten", procargs2(3, "/bin/node\x00\x00\x00claude\x00\x00\x00\x00\x00\x00\x00\x00ptr_munge=\x00"), []string{"claude"}, nil},
		{"empty", nil, nil, nil},
		{"short", []byte{2, 0, 0}, nil, nil},
	} {
		if argv, env := parse(tc.b); !slices.Equal(argv, tc.argv) || !slices.Equal(env, tc.env) {
			t.Errorf("%s: parse = %q, %q; want %q, %q", tc.name, argv, env, tc.argv, tc.env)
		}
	}
}
