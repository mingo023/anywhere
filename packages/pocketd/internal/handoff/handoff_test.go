package handoff

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"golang.org/x/sys/unix"

	"pocketd/internal/state"
)

func TestAFileReadsBackAsWritten(t *testing.T) {
	path := filepath.Join(t.TempDir(), "handoff.json")
	f := File{Format: Format, Terminals: []Terminal{{
		Saved: state.Terminal{TerminalID: "t-1", LaunchDir: "/w", Cols: 80, Rows: 24, Provider: "claude", ConversationID: "c1", AgentID: "a-1"},
		Cmd:   "zsh", Args: []string{"-l"}, Cwd: "/w", FD: 7, Pid: 42, Screen: []byte("\x1b[Hhi"),
		Agent: &Agent{Named: "Fix login", Phase: "working", Epoch: 2, Seq: 9},
	}}}
	if err := Write(path, f); err != nil {
		t.Fatal(err)
	}
	got, err := Read(path)
	if err != nil || !reflect.DeepEqual(got, f) {
		t.Fatalf("read %+v, %v", got, err)
	}
}

func TestTheFirstReleasesFileStillReads(t *testing.T) {
	f, err := Read("testdata/format1.json")
	if err != nil {
		t.Fatal(err)
	}
	term := f.Terminals[0]
	if term.Saved.TerminalID != "t-1" || term.Saved.AgentID != "a-1" || term.FD != 7 || term.Pid != 42 ||
		string(term.Screen) != "hi" || term.Agent.Named != "Fix login" || term.Agent.Seq != 9 {
		t.Fatalf("format 1 = %+v", f)
	}
}

func TestANewerFormatIsRefused(t *testing.T) {
	path := filepath.Join(t.TempDir(), "handoff.json")
	os.WriteFile(path, []byte(`{"format":99,"terminals":[]}`), 0o600)
	if _, err := Read(path); err == nil || !strings.Contains(err.Error(), "format 99") {
		t.Fatalf("err = %v", err)
	}
}

func TestCheckRefusesAClosedFD(t *testing.T) {
	err := Check(File{Format: Format, Terminals: []Terminal{{Saved: state.Terminal{TerminalID: "t-1"}, FD: 65000}}})
	if err == nil || !strings.Contains(err.Error(), "t-1") {
		t.Fatalf("err = %v", err)
	}
}

func TestCheckRefusesAnFDThatClosesOnExec(t *testing.T) {
	devnull, err := os.Open(os.DevNull)
	if err != nil {
		t.Fatal(err)
	}
	defer devnull.Close()
	f := File{Format: Format, Terminals: []Terminal{{Saved: state.Terminal{TerminalID: "t-1"}, FD: int(devnull.Fd())}}}
	if Check(f) == nil {
		t.Fatal("an fd that closes on exec passed")
	}
	unix.FcntlInt(devnull.Fd(), unix.F_SETFD, 0)
	if err := Check(f); err != nil {
		t.Fatal(err)
	}
}
