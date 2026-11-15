//go:build unix

package factory

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
	"unicode/utf8"

	"pocketd/internal/proto"
)

func TestATicketWithNoCheckRecordHasNone(t *testing.T) {
	if r, ok := ReadRecord(t.TempDir(), "t1"); ok {
		t.Fatalf("got %+v", r)
	}
}

func TestTheNewestCheckRecordIsTheOneThatIsRead(t *testing.T) {
	home := t.TempDir()
	first := Record{Head: "aaa", State: LocalFailed, Total: 1, Checks: []proto.FactoryCheck{{Command: "make test", Exit: 2, Output: "boom\n", Attempt: 0, At: 5}}}
	if err := WriteRecord(home, "t1", first, true); err != nil {
		t.Fatal(err)
	}
	first.Fix = FixMailed
	if err := WriteRecord(home, "t1", first, false); err != nil {
		t.Fatal(err)
	}
	if err := WriteRecord(home, "t1", Record{Head: "bbb"}, true); err != nil {
		t.Fatal(err)
	}
	got, ok := ReadRecord(home, "t1")
	if !ok || got.Head != "bbb" {
		t.Fatalf("got %+v", got)
	}
	names, _ := filepath.Glob(filepath.Join(home, "state", "factory", "t1", "checks", "*.json"))
	if len(names) != 2 {
		t.Fatalf("a save that is not fresh must replace the newest file, got %v", names)
	}
	raw, _ := os.ReadFile(names[0])
	if want := `{"head":"aaa","state":"failed","total":1,"checks":[{"command":"make test","exit":2,"output":"boom\n","attempt":0,"at":5}],"fix":"mailed"}`; string(raw) != want {
		t.Fatalf("got  %s\nwant %s", raw, want)
	}
}

func TestACheckRecordThatCantBeReadCountsAsNone(t *testing.T) {
	home := t.TempDir()
	dir := filepath.Join(home, "state", "factory", "t1", "checks")
	os.MkdirAll(dir, 0o700)
	os.WriteFile(filepath.Join(dir, "0001.json"), []byte("{"), 0o600)
	if r, ok := ReadRecord(home, "t1"); ok || !reflect.DeepEqual(r, Record{}) {
		t.Fatalf("got %+v", r)
	}
}

func TestThePRBodyIsTheBriefAndTheCriteriaThenTheKey(t *testing.T) {
	spec := "## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n- [ ] It is off by default\n\n## Risks\n- None\n"
	want := "## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n- [ ] It is off by default\n\nTicket APP-12"
	if got := PRBody(spec, "APP-12"); got != want {
		t.Fatalf("got  %q\nwant %q", got, want)
	}
}

func TestATicketWithoutASpecHasOnlyItsKeyInThePRBody(t *testing.T) {
	if got := PRBody("", "APP-12"); got != "Ticket APP-12" {
		t.Fatalf("got %q", got)
	}
}

func TestATicketWithoutAKeyOrASpecHasAnEmptyPRBody(t *testing.T) {
	if got := PRBody("", ""); got != "" {
		t.Fatalf("got %q", got)
	}
}

func TestAShortOutputIsKeptWhole(t *testing.T) {
	if got := Tail([]byte("one\ntwo\n"), 64); got != "one\ntwo\n" {
		t.Fatalf("got %q", got)
	}
}

func TestALongOutputKeepsItsLastLinesFromALineStart(t *testing.T) {
	out := strings.Repeat("0123456789\n", 10)
	if got := Tail([]byte(out), 25); got != "0123456789\n0123456789\n" {
		t.Fatalf("got %q", got)
	}
}

func TestOneLineLongerThanTheLimitIsCutToValidText(t *testing.T) {
	got := Tail([]byte(strings.Repeat("é", 100)), 15)
	if !utf8.ValidString(got) || len(got) > 15 || len(got) < 13 {
		t.Fatalf("got %q (%d bytes)", got, len(got))
	}
}

var plain = []string{"PATH=/usr/bin:/bin", "GREETING=hello"}

func TestACommandThatExitsZeroPasses(t *testing.T) {
	if exit, out := RunCheck(context.Background(), plain, t.TempDir(), "true", time.Minute); exit != 0 || out != "" {
		t.Fatalf("got %d %q", exit, out)
	}
}

func TestACommandThatExitsOneFails(t *testing.T) {
	if exit, _ := RunCheck(context.Background(), plain, t.TempDir(), "false", time.Minute); exit != 1 {
		t.Fatalf("got %d", exit)
	}
}

func TestACommandsExitCodeAndOutputAreKept(t *testing.T) {
	exit, out := RunCheck(context.Background(), plain, t.TempDir(), "sh -c 'echo x; exit 3'", time.Minute)
	if exit != 3 || out != "x\n" {
		t.Fatalf("got %d %q", exit, out)
	}
}

func TestStderrIsKeptWithStdoutInTheOrderWritten(t *testing.T) {
	_, out := RunCheck(context.Background(), plain, t.TempDir(), "echo one; echo two >&2; echo three", time.Minute)
	if out != "one\ntwo\nthree\n" {
		t.Fatalf("got %q", out)
	}
}

func TestACommandRunsInTheCheckoutWithTheEnvironmentItIsGiven(t *testing.T) {
	dir, _ := filepath.EvalSymlinks(t.TempDir())
	_, out := RunCheck(context.Background(), plain, dir, "pwd; echo $GREETING", time.Minute)
	if out != dir+"\nhello\n" {
		t.Fatalf("got %q", out)
	}
}

func TestACommandThatRunsTooLongIsStoppedAndFails(t *testing.T) {
	exit, out := RunCheck(context.Background(), plain, t.TempDir(), "echo started; exec sleep 30", 200*time.Millisecond)
	if exit != -1 || !strings.HasPrefix(out, "started\n") || !strings.HasSuffix(out, "Stopped after 200ms.\n") {
		t.Fatalf("got %d %q", exit, out)
	}
}

func TestOnlyTheEndOfALongOutputIsKept(t *testing.T) {
	_, out := RunCheck(context.Background(), plain, t.TempDir(), "i=0; while [ $i -lt 9000 ]; do echo 0123456789; i=$((i+1)); done; echo last", time.Minute)
	if len(out) > OutputLimit || !strings.HasSuffix(out, "0123456789\nlast\n") || !strings.HasPrefix(out, "0123456789\n") {
		t.Fatalf("kept %d bytes", len(out))
	}
}
