package ops

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/terminal"
)

func sendRaw(t *testing.T, c *Conn, line string) {
	t.Helper()
	if _, err := fmt.Fprintln(c.conn, line); err != nil {
		t.Fatal(err)
	}
}

func TestAReadTheClientHoldsComesBackUnchangedUntilTheFileChanges(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	f := filepath.Join(t.TempDir(), "f.txt")
	os.WriteFile(f, []byte("one"), 0o600)
	read := func(id string, cached bool) Msg {
		c.Send(Msg{Op: "fs.read", ID: id, Path: f, Max: 1 << 20, Cached: cached})
		return recv(t, c, "fs.read")
	}
	if m := read("1", false); string(m.Data) != "one" || m.Unchanged {
		t.Fatalf("%+v", m)
	}
	if m := read("2", true); !m.Unchanged || m.ID != "2" || m.Data != nil {
		t.Fatalf("%+v", m)
	}
	os.WriteFile(f, []byte("two"), 0o600)
	if m := read("3", true); string(m.Data) != "two" || m.Unchanged || m.ID != "3" {
		t.Fatalf("%+v", m)
	}
	if m := read("4", true); !m.Unchanged {
		t.Fatalf("%+v", m)
	}
}

func TestWithoutCachedTheReplyIsAlwaysWhole(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	for _, id := range []string{"1", "2"} {
		c.Send(Msg{Op: "exec", ID: id, Cwd: t.TempDir(), Argv: []string{"echo", "hi"}})
		if m := recv(t, c, "exec"); string(m.Stdout) != "hi\n" || m.Unchanged {
			t.Fatalf("%+v", m)
		}
	}
}

func TestErrorsAndWritesAreNeverUnchanged(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	missing := filepath.Join(t.TempDir(), "missing")
	for _, id := range []string{"1", "2"} {
		c.Send(Msg{Op: "fs.read", ID: id, Path: missing, Cached: true})
		if m := recv(t, c, "error"); m.Unchanged || m.Error == "" {
			t.Fatalf("%+v", m)
		}
	}
	dir := filepath.Join(t.TempDir(), "d")
	for _, id := range []string{"1", "2"} {
		c.Send(Msg{Op: "fs.mkdir", ID: id, Path: dir, Parents: true, Cached: true})
		if m := recv(t, c, "fs.mkdir"); m.Unchanged {
			t.Fatalf("%+v", m)
		}
	}
}

func TestARequestThatDiffersOnlyInAFieldPocketdIgnoresIsAnotherRequest(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	f := filepath.Join(t.TempDir(), "f.txt")
	os.WriteFile(f, []byte("old"), 0o600)
	sendRaw(t, c, fmt.Sprintf(`{"op":"fs.read","id":"1","path":%q,"note":"b"}`, f))
	recv(t, c, "fs.read")
	os.WriteFile(f, []byte("new"), 0o600)
	sendRaw(t, c, fmt.Sprintf(`{"op":"fs.read","id":"2","path":%q,"note":"a"}`, f))
	recv(t, c, "fs.read")
	sendRaw(t, c, fmt.Sprintf(`{"op":"fs.read","id":"3","path":%q,"note":"b","cached":true}`, f))
	if m := recv(t, c, "fs.read"); m.Unchanged || string(m.Data) != "new" {
		t.Fatalf("the reply the client holds for this request is %q: %+v", "old", m)
	}
}

func TestListIsUnchangedWhileTheTerminalsAre(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	c.Send(Msg{Op: "list"})
	recv(t, c, "terminals")
	c.Send(Msg{Op: "list", Cached: true})
	if m := recv(t, c, "terminals"); !m.Unchanged {
		t.Fatalf("%+v", m)
	}
	c.Send(Msg{Op: "spawn", Cmd: "sh", Args: []string{"-c", "sleep 5"}})
	spawned := recv(t, c, "spawned")
	t.Cleanup(func() { c.Send(Msg{Op: "close", ID: spawned.ID}) })
	c.Send(Msg{Op: "list", Cached: true})
	if m := recv(t, c, "terminals"); m.Unchanged || len(m.Items) != 1 {
		t.Fatalf("%+v", m)
	}
}

func TestTheTableDropsTheEntryWrittenLongestAgo(t *testing.T) {
	r := newSentHashes()
	key := func(i int) [32]byte { return [32]byte{byte(i), byte(i >> 8)} }
	sum := [32]byte{1}
	for i := range maxReplies {
		r.repeats(key(i), sum)
	}
	r.repeats(key(0), sum)
	r.repeats(key(maxReplies), sum)
	if !r.repeats(key(0), sum) {
		t.Error("the entry written last was dropped")
	}
	if r.repeats(key(1), sum) {
		t.Error("the entry written longest ago was kept")
	}
	if len(r.at) != maxReplies {
		t.Errorf("%d entries", len(r.at))
	}
}
