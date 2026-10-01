package proc

import (
	"os"
	"os/exec"
	"slices"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/creack/pty"
)

func TestSessionListsAJobInItsOwnGroup(t *testing.T) {
	cmd := exec.Command("sh", "-i")
	cmd.Env = []string{"PATH=/bin:/usr/bin", "PS1=$ "}
	f, err := pty.Start(cmd)
	if err != nil {
		t.Fatal(err)
	}
	sid := cmd.Process.Pid
	t.Cleanup(func() {
		pgids, _ := Session(sid)
		for _, pg := range pgids {
			syscall.Kill(-pg, syscall.SIGKILL)
		}
		cmd.Wait()
		f.Close()
	})
	go func() {
		buf := make([]byte, 1024)
		for {
			if _, err := f.Read(buf); err != nil {
				return
			}
		}
	}()
	f.WriteString("sleep 30 &\n")
	var pgids []int
	eventually(t, "two groups", func() bool {
		pgids, _ = Session(sid)
		return len(pgids) == 2
	})
	if !slices.Contains(pgids, sid) {
		t.Fatalf("Session(%d) = %v, want the shell's group and the job's", sid, pgids)
	}
}

func TestOrphansSeeAnAdoptedChildWithItsMarker(t *testing.T) {
	// The marked parent (go test) is alive, so the scratch pocketds that e2e
	// starts meanwhile spare the orphan; only this Reap is told it is dead.
	parent := os.Getppid()
	sh := exec.Command("sh", "-c", `"$0" -test.run='^TestHelperProcess$' >/dev/null 2>&1 & echo $!`, os.Args[0])
	sh.Env = []string{"PATH=/bin:/usr/bin", "POCKET_TEST=1", Marker + "=" + strconv.Itoa(parent)}
	out, err := sh.Output()
	if err != nil {
		t.Fatal(err)
	}
	pid, _ := strconv.Atoi(strings.TrimSpace(string(out)))
	t.Cleanup(func() { syscall.Kill(pid, syscall.SIGKILL) })

	var mine []Proc
	eventually(t, "the orphan with its marker", func() bool {
		ps, _ := Orphans(os.Getuid())
		mine = slices.DeleteFunc(ps, func(p Proc) bool { return p.Pid != pid })
		// Until it execs, the forked child reads back with sh's argv and no env.
		return len(mine) == 1 && markedBy(mine[0].Env) == parent
	})
	if got := Reap(os.Getpid(), dead, mine); !slices.Equal(got, []int{pid}) {
		t.Fatalf("Reap = %v, want [%d]", got, pid)
	}
	Kill([]int{pid}, time.Second)
	eventually(t, "the orphan gone", func() bool { return !Alive(pid) })
}
