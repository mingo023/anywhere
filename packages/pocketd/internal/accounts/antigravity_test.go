package accounts

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"
)

type fakeAgy struct {
	mu      sync.Mutex
	runs    [][]string
	version string
	usage   agyRun
	err     error
	hang    bool
}

func (f *fakeAgy) run(ctx context.Context, exe string, env []string, dir string, args ...string) (agyRun, error) {
	f.mu.Lock()
	f.runs = append(f.runs, args)
	version, usage, err, hang := f.version, f.usage, f.err, f.hang
	f.mu.Unlock()
	if args[0] == "--version" {
		return agyRun{out: []byte(version)}, nil
	}
	if hang {
		<-ctx.Done()
		return agyRun{}, ctx.Err()
	}
	time.Sleep(20 * time.Millisecond)
	return usage, err
}

func (f *fakeAgy) set(usage agyRun, err error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.usage, f.err = usage, err
}

func (f *fakeAgy) count(arg string) int {
	f.mu.Lock()
	defer f.mu.Unlock()
	n := 0
	for _, r := range f.runs {
		if r[0] == arg {
			n++
		}
	}
	return n
}

// The report in testdata is derived from research doc §2.5, not captured from a live agy.
func report(t *testing.T) []byte {
	raw, err := os.ReadFile(filepath.Join("testdata", "agy-usage.json"))
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

func agyStore(t *testing.T, f *fakeAgy) (*Store, string) {
	s := store(t)
	exe := filepath.Join(t.TempDir(), "agy")
	if err := os.WriteFile(exe, nil, 0o755); err != nil {
		t.Fatal(err)
	}
	s.Agy = func() (string, []string, bool) { return exe, nil, true }
	s.runAgy = f.run
	return s, exe
}

func agyRow(t *testing.T, s *Store) Account {
	t.Helper()
	for _, a := range s.List(none) {
		if a.Provider == "antigravity" {
			return a
		}
	}
	t.Fatal("no Antigravity row")
	return Account{}
}

func refresh(s *Store) { s.Refresh(context.Background(), "") }

func floorPassed(s *Store) {
	s.agy.mu.Lock()
	s.agy.at = s.agy.at.Add(-agyFloor)
	s.agy.mu.Unlock()
}

func noMeters(a Account) bool {
	return a.Usage != nil && a.Usage.At > 0 && a.Usage.Session == nil && a.Usage.Week == nil && !a.Usage.Stale
}

func TestTheUsageReportFillsTheSessionAndWeekFromGeminisPools(t *testing.T) {
	f := &fakeAgy{version: "agy 1.5.16\n", usage: agyRun{out: report(t)}}
	s, _ := agyStore(t, f)
	refresh(s)
	a := agyRow(t, s)
	if !a.SignedIn || a.Error != "" || a.Usage == nil {
		t.Fatalf("%+v", a)
	}
	if *a.Usage.Session != (Window{Percent: 100, ResetsAt: 1795456800000}) || *a.Usage.Week != (Window{Percent: 25}) {
		t.Fatalf("session %+v, week %+v", a.Usage.Session, a.Usage.Week)
	}
	args := f.runs[1]
	want := []string{"--print", "/usage", "--output-format", "json", "--print-timeout", "10s", "--settings", `{"disableSlashCommands":false}`, "--disable-slash-commands=false", "--log-file"}
	if !reflect.DeepEqual(f.runs[0], []string{"--version"}) || !reflect.DeepEqual(args[:len(want)], want) || len(args) != len(want)+1 || filepath.Base(args[len(want)]) != "agy.log" {
		t.Fatalf("ran %q", f.runs)
	}
}

func TestADisabledOrUnreadBucketShowsNoMeter(t *testing.T) {
	out := `{"status":"SUCCESS","num_turns":0,"command":{"name":"usage","data":{"groups":[{"name":"gemini","buckets":[
		{"window":"5h","disabled":true,"remaining_fraction":0.5},{"window":"weekly"}]}]}}}`
	s, _ := agyStore(t, &fakeAgy{version: "1.1.11", usage: agyRun{out: []byte(out)}})
	refresh(s)
	if a := agyRow(t, s); !a.SignedIn || !noMeters(a) || a.Error != "" {
		t.Fatalf("%+v %+v", a, a.Usage)
	}
}

func TestOutputThatIsNotTheCLIsOwnReportIsRejected(t *testing.T) {
	for name, out := range map[string]string{
		"a model answer": `{"status":"SUCCESS","num_turns":1,"result":"Your usage is fine","command":{"name":"usage"}}`,
		"no turn count":  `{"status":"SUCCESS","command":{"name":"usage"}}`,
		"another report": `{"status":"SUCCESS","num_turns":0,"command":{"name":"quota"}}`,
		"plain text":     "You have used 20% of your quota.",
		"nothing":        "",
	} {
		f := &fakeAgy{version: "1.5.16", usage: agyRun{out: []byte(out)}}
		s, _ := agyStore(t, f)
		refresh(s)
		if a := agyRow(t, s); !a.SignedIn || !noMeters(a) || a.Error != agyOldNote {
			t.Errorf("%s: %+v %+v", name, a, a.Usage)
		}
		floorPassed(s)
		refresh(s)
		if n := f.count("--print"); n != 1 {
			t.Errorf("%s: asked %d times", name, n)
		}
	}
}

func TestAnAgyOlderThanTheGateIsNeverAskedForItsUsage(t *testing.T) {
	f := &fakeAgy{version: "agy version 1.1.10", usage: agyRun{out: report(t)}}
	s, exe := agyStore(t, f)
	refresh(s)
	if a := agyRow(t, s); !a.SignedIn || !noMeters(a) || a.Error != agyOldNote {
		t.Fatalf("%+v", a)
	}
	floorPassed(s)
	refresh(s)
	if !reflect.DeepEqual(f.runs, [][]string{{"--version"}}) {
		t.Fatalf("ran %q", f.runs)
	}
	later := time.Now().Add(time.Hour)
	if err := os.Chtimes(exe, later, later); err != nil {
		t.Fatal(err)
	}
	refresh(s)
	if f.count("--version") != 2 || f.count("--print") != 0 {
		t.Fatalf("after an update, ran %q", f.runs)
	}
	moved := filepath.Join(filepath.Dir(exe), "agy2")
	os.Rename(exe, moved)
	s.Agy = func() (string, []string, bool) { return moved, nil, true }
	refresh(s)
	if f.count("--version") != 3 {
		t.Fatalf("after a move, ran %q", f.runs)
	}
}

func TestAVersionAgyCantSayCountsAsTooOld(t *testing.T) {
	f := &fakeAgy{version: "agy (dev build)", usage: agyRun{out: report(t)}}
	s, _ := agyStore(t, f)
	refresh(s)
	if a := agyRow(t, s); a.Error != agyOldNote || f.count("--print") != 0 {
		t.Fatalf("%+v, ran %q", a, f.runs)
	}
	for out, want := range map[string]bool{"1.1.11": true, "1.1.12": true, "1.2.0": true, "2.0.0": true, "agy 10.0.0 (abc)": true, "1.1.10": false, "1.0.99": false, "0.9.20": false, "1.1": false, "": false, "99999999999999999999.0.0": false} {
		if got := newEnough([]byte(out)); got != want {
			t.Errorf("%q: %v", out, got)
		}
	}
}

func TestTheSignInLineMarksAntigravitySignedOutAndASignInEndingClearsIt(t *testing.T) {
	f := &fakeAgy{version: "1.5.16", usage: agyRun{out: report(t)}}
	s, _ := agyStore(t, f)
	refresh(s)
	floorPassed(s)
	f.set(agyRun{signIn: true}, nil)
	refresh(s)
	if a := agyRow(t, s); a.SignedIn || !noMeters(a) || a.Error != "" {
		t.Fatalf("%+v %+v", a, a.Usage)
	}
	f.set(agyRun{out: report(t)}, nil)
	refresh(s)
	if f.count("--print") != 2 {
		t.Fatalf("probed again inside the floor: %q", f.runs)
	}
	s.Finish("antigravity", BuiltIn, func() {})
	refresh(s)
	if a := agyRow(t, s); !a.SignedIn || a.Usage.Session == nil || f.count("--print") != 3 {
		t.Fatalf("after a sign-in: %+v, ran %q", a, f.runs)
	}
}

func TestAntigravityIsProbedAtMostOncePerFloor(t *testing.T) {
	f := &fakeAgy{version: "1.5.16", usage: agyRun{out: report(t)}}
	s, _ := agyStore(t, f)
	refresh(s)
	refresh(s)
	if n := f.count("--print"); n != 1 {
		t.Fatalf("%d runs in a row", n)
	}
	if a := agyRow(t, s); a.Usage == nil || a.Usage.Stale || a.Usage.Session == nil {
		t.Fatalf("a cached hit: %+v", a.Usage)
	}

	f = &fakeAgy{version: "1.5.16", usage: agyRun{out: report(t)}}
	s, _ = agyStore(t, f)
	var wg sync.WaitGroup
	wg.Go(func() { refresh(s) })
	wg.Go(func() { refresh(s) })
	wg.Wait()
	if n := f.count("--print"); n != 1 {
		t.Fatalf("%d runs at once", n)
	}
	floorPassed(s)
	refresh(s)
	if n := f.count("--print"); n != 2 {
		t.Fatalf("%d runs once the floor passed", n)
	}
}

func TestAProbeTheCallerCancelledKeepsTheLastReadFresh(t *testing.T) {
	f := &fakeAgy{version: "1.5.16", usage: agyRun{out: report(t)}}
	s, _ := agyStore(t, f)
	refresh(s)
	floorPassed(s)
	f.mu.Lock()
	f.hang = true
	f.mu.Unlock()
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	s.Refresh(ctx, "")
	cancel()
	if a := agyRow(t, s); !a.SignedIn || a.Usage == nil || a.Usage.Stale || a.Usage.Session == nil || a.Usage.Session.Percent != 100 {
		t.Fatalf("%+v", a.Usage)
	}
	f.mu.Lock()
	f.hang = false
	f.mu.Unlock()
	refresh(s)
	if n := f.count("--print"); n != 2 {
		t.Fatalf("probed again inside the floor: %d runs", n)
	}
}

func TestALaterOldOrNoDataOutcomeDropsTheEarlierMeters(t *testing.T) {
	for name, later := range map[string]agyRun{
		"not its report": {out: []byte("Sure, here is your usage.")},
		"an API key":     {out: []byte("Error: usage needs a Google sign-in"), code: 2},
		"signed out":     {signIn: true},
	} {
		f := &fakeAgy{version: "1.5.16", usage: agyRun{out: report(t)}}
		s, _ := agyStore(t, f)
		refresh(s)
		floorPassed(s)
		f.set(later, nil)
		refresh(s)
		if a := agyRow(t, s); !noMeters(a) {
			t.Errorf("%s: %+v", name, a.Usage)
		}
	}
}

func TestAFailedReadLeavesTheLastUsageStale(t *testing.T) {
	for name, fail := range map[string]struct {
		run agyRun
		err error
	}{
		"another exit": {run: agyRun{out: []byte("panic"), code: 1}},
		"no start":     {err: errors.New("exec: permission denied")},
	} {
		f := &fakeAgy{version: "1.5.16", usage: agyRun{out: report(t)}}
		s, _ := agyStore(t, f)
		refresh(s)
		floorPassed(s)
		f.set(fail.run, fail.err)
		refresh(s)
		if a := agyRow(t, s); !a.SignedIn || a.Usage == nil || !a.Usage.Stale || a.Usage.Session.Percent != 100 {
			t.Errorf("%s: %+v %+v", name, a, a.Usage)
		}
	}
}

func TestWithoutAgyTheRowIsSignedOutAndNothingRuns(t *testing.T) {
	for name, find := range map[string]func() (string, []string, bool){
		"no finder": nil,
		"not found": func() (string, []string, bool) { return "", nil, false },
	} {
		s := store(t)
		s.Agy = find
		s.runAgy = func(context.Context, string, []string, string, ...string) (agyRun, error) {
			t.Errorf("%s: ran agy", name)
			return agyRun{}, nil
		}
		refresh(s)
		if a := agyRow(t, s); a.SignedIn || a.Error != "" || a.Default != true {
			t.Errorf("%s: %+v", name, a)
		}
	}
}

func TestAGeminiAPIKeyExitLeavesTheRowSignedInWithoutUsage(t *testing.T) {
	s, _ := agyStore(t, &fakeAgy{version: "1.5.16", usage: agyRun{out: []byte("usage needs a Google sign-in"), code: 2}})
	refresh(s)
	if a := agyRow(t, s); !a.SignedIn || !noMeters(a) || a.Error != "" {
		t.Fatalf("%+v %+v", a, a.Usage)
	}
}

// fakeAgyScript is a real agy stand-in: it says its version, and answers anything else with body, exiting with code.
func fakeAgyScript(t *testing.T, body string, code int) string {
	if runtime.GOOS == "windows" {
		t.Skip("needs /bin/sh")
	}
	exe := filepath.Join(t.TempDir(), "agy")
	script := fmt.Sprintf("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 1.5.16; exit 0; fi\n%s\nexit %d\n", body, code)
	if err := os.WriteFile(exe, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	return exe
}

func TestNothingAgyPrintedReachesAnError(t *testing.T) {
	const url = "https://accounts.example/login?code=SECRET"
	for _, code := range []int{0, 1, 2} {
		exe := fakeAgyScript(t, "echo "+url+"; echo "+url+" >&2", code)
		s := store(t)
		s.Agy = func() (string, []string, bool) { return exe, nil, true }
		_, err := s.fetch(context.Background(), "antigravity", BuiltIn)
		if err != nil && strings.Contains(err.Error(), "SECRET") {
			t.Errorf("exit %d: %v", code, err)
		}
		s.Refresh(context.Background(), "")
		if got := fmt.Sprintf("%+v", agyRow(t, s)); strings.Contains(got, "SECRET") {
			t.Errorf("exit %d: %s", code, got)
		}
	}
}

func TestTheSignInWatchStopsAgyOnTheSignInLine(t *testing.T) {
	cancelled := false
	w := &signInWatch{cancel: func() { cancelled = true }}
	w.Write([]byte("Loading…\nAuthentication req"))
	if cancelled || w.signIn {
		t.Fatal("stopped before the line was whole")
	}
	w.Write([]byte("uired. Please visit https://accounts.example/login?code=SECRET\nmore\n"))
	if !cancelled || !w.signIn || len(w.line) != 0 {
		t.Fatalf("cancelled %v, kept %q", cancelled, w.line)
	}
}

func TestTheRealRunnerKillsAgyAtTheSignInLine(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("needs /bin/sh")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	start := time.Now()
	r, err := runAgyProcess(ctx, "/bin/sh", nil, t.TempDir(), "-c", `echo "Authentication required. Please visit the URL to log in" >&2; sleep 30`)
	if err != nil || !r.signIn || time.Since(start) > 3*time.Second {
		t.Fatalf("%+v, %v after %v", r, err, time.Since(start))
	}
}

func TestTheRealRunnerKeepsAtMostAMebibyteOfStdout(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("needs /bin/sh")
	}
	r, err := runAgyProcess(context.Background(), "/bin/sh", nil, t.TempDir(), "-c", "head -c 3000000 /dev/zero")
	if err != nil || len(r.out) != 1<<20 {
		t.Fatalf("kept %d bytes, %v", len(r.out), err)
	}
}

func TestASignInEndingDuringAProbeStillLetsTheNextRefreshProbe(t *testing.T) {
	release := make(chan struct{})
	f := &fakeAgy{version: "1.5.16", usage: agyRun{signIn: true}}
	s, _ := agyStore(t, f)
	s.runAgy = func(ctx context.Context, exe string, env []string, dir string, args ...string) (agyRun, error) {
		if args[0] == "--print" && f.count("--print") == 0 {
			<-release
		}
		return f.run(ctx, exe, env, dir, args...)
	}
	done := make(chan struct{})
	go func() { refresh(s); close(done) }()
	// Until the probe holds the lock.
	for s.agy.run.TryLock() {
		s.agy.run.Unlock()
		time.Sleep(time.Millisecond)
	}
	s.Finish("antigravity", BuiltIn, func() {})
	close(release)
	<-done
	f.set(agyRun{out: report(t)}, nil)
	refresh(s)
	if a := agyRow(t, s); !a.SignedIn || f.count("--print") != 2 {
		t.Fatalf("%+v, ran %q", a, f.runs)
	}
}

// agy-signed-out.json is what a signed-out agy 1.5.16 printed on stdout for /usage, captured live. It exits 0.
func signedOutReport(t *testing.T) []byte {
	raw, err := os.ReadFile(filepath.Join("testdata", "agy-signed-out.json"))
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

func TestASignedOutAgysErrorReportMarksTheRowSignedOut(t *testing.T) {
	f := &fakeAgy{version: "1.5.16", usage: agyRun{out: signedOutReport(t)}}
	s, _ := agyStore(t, f)
	refresh(s)
	if a := agyRow(t, s); a.SignedIn || a.Error != "" || !noMeters(a) {
		t.Fatalf("%+v %+v", a, a.Usage)
	}
	s.Finish("antigravity", BuiltIn, func() {})
	f.set(agyRun{out: report(t)}, nil)
	refresh(s)
	if a := agyRow(t, s); !a.SignedIn || a.Usage.Session == nil {
		t.Fatalf("after a sign-in: %+v", a)
	}
}

func TestAnErrorReportBeforeAnyTurnIsAFailureTriedAgainAfterTheFloor(t *testing.T) {
	f := &fakeAgy{version: "1.5.16", usage: agyRun{out: report(t)}}
	s, _ := agyStore(t, f)
	refresh(s)
	floorPassed(s)
	f.set(agyRun{out: []byte(`{"status":"ERROR","error":"network unreachable","num_turns":0}`)}, nil)
	refresh(s)
	if a := agyRow(t, s); !a.SignedIn || a.Error != "" || !a.Usage.Stale || a.Usage.Session.Percent != 100 {
		t.Fatalf("%+v %+v", a, a.Usage)
	}
	floorPassed(s)
	f.set(agyRun{out: report(t)}, nil)
	refresh(s)
	if a := agyRow(t, s); a.Usage.Stale || f.count("--print") != 3 {
		t.Fatalf("not tried again: %+v, ran %q", a.Usage, f.runs)
	}
}

// The sign-in text was seen on a terminal, so which stream carries it is unknown: each must stop agy.
func TestTheRealRunnerStopsAgyAtTheSignInTextOnEitherStream(t *testing.T) {
	for _, stream := range []string{"", " >&2"} {
		exe := fakeAgyScript(t, `echo "Authentication required. Please visit the URL to log in:"`+stream+`; sleep 30`, 0)
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		r, err := runAgyProcess(ctx, exe, nil, t.TempDir(), "--print", "/usage")
		cancel()
		if err != nil || !r.signIn {
			t.Errorf("stream %q: %+v, %v", stream, r, err)
		}
	}
}
