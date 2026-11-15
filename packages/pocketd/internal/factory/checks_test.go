//go:build unix

package factory

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

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
