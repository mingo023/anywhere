package automation

import (
	"database/sql"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/statedb/dbtest"
)

// newDB is a database whose registry lists the projects /f and /other.
func newDB(t *testing.T) *sql.DB {
	t.Helper()
	db := dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
	tx, err := db.Begin()
	if err != nil {
		t.Fatal(err)
	}
	if err := registry.Insert(tx, registry.State{Projects: []proto.RegistryProject{{ID: "p1", Path: "/f"}, {ID: "p2", Path: "/other"}}}); err != nil {
		t.Fatal(err)
	}
	if err := tx.Commit(); err != nil {
		t.Fatal(err)
	}
	return db
}

func open(t *testing.T, db *sql.DB) *Store {
	t.Helper()
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	s, err := Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	return s
}

func openNew(t *testing.T) *Store { return open(t, newDB(t)) }

var morning = proto.Automation{Name: "Morning review", Prompt: "Review new PRs", Provider: "claude", Folder: "/f",
	Schedule: proto.Schedule{Kind: "days", Days: []int{1, 2, 3, 4, 5}, Time: "09:00"}, Enabled: true}

func saved(t *testing.T, s *Store, now time.Time) proto.Automation {
	t.Helper()
	a, err := s.Save(morning, now)
	if err != nil {
		t.Fatal(err)
	}
	return a
}

func TestWhatIsSavedLoadsBackUnchanged(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	a := saved(t, s, at(9, 8, 0))
	run, err := s.Begin(a.ID, at(9, 8, 5))
	if err != nil {
		t.Fatal(err)
	}
	autos, runs := open(t, db).Snapshot()
	if !reflect.DeepEqual(autos, []proto.Automation{a}) || !slices.Equal(runs, []proto.Run{run}) {
		t.Fatalf("got %+v %+v", autos, runs)
	}
}

func TestSavingGivesANewIDAndSchedulesTheNextRun(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, at(9, 8, 0))
	if a.ID == "" || a.NextRunAt != at(9, 9, 0).UnixMilli() {
		t.Fatalf("%+v", a)
	}
	b := saved(t, s, at(9, 8, 0))
	if b.ID == a.ID {
		t.Fatal("two automations share an ID")
	}
}

func TestSavingAgainReplacesTheAutomationAndKeepsItsRuns(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, at(9, 8, 0))
	s.Begin(a.ID, at(9, 8, 1))
	a.Name, a.Schedule = "Evening review", proto.Schedule{Kind: "interval", EveryMin: 60}
	got, err := s.Save(a, at(9, 8, 30))
	if err != nil {
		t.Fatal(err)
	}
	autos, runs := s.Snapshot()
	if len(autos) != 1 || !reflect.DeepEqual(autos[0], got) || got.NextRunAt != at(9, 9, 30).UnixMilli() || len(runs) != 1 {
		t.Fatalf("%+v %+v", autos, runs)
	}
}

func TestSavingRefusesWhatIsInvalidOrUnknown(t *testing.T) {
	s := openNew(t)
	bad := morning
	bad.Name = " "
	if _, err := s.Save(bad, at(9, 8, 0)); err != ErrInvalid {
		t.Fatalf("got %v", err)
	}
	gone := morning
	gone.ID = "nope"
	if _, err := s.Save(gone, at(9, 8, 0)); err != ErrUnknown {
		t.Fatalf("got %v", err)
	}
}

func TestAPausedAutomationHasNoNextRunUntilItIsEnabledAgain(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, at(9, 8, 0))
	if err := s.Enable(a.ID, false, at(9, 8, 1)); err != nil {
		t.Fatal(err)
	}
	if got, _ := s.Automation(a.ID); got.Enabled || got.NextRunAt != 0 {
		t.Fatalf("%+v", got)
	}
	if err := s.Enable(a.ID, true, at(9, 10, 0)); err != nil {
		t.Fatal(err)
	}
	if got, _ := s.Automation(a.ID); !got.Enabled || got.NextRunAt != at(12, 9, 0).UnixMilli() {
		t.Fatalf("%+v", got)
	}
	if s.Enable("nope", true, at(9, 10, 0)) != ErrUnknown {
		t.Fatal("enabled an unknown automation")
	}
}

func TestOnlyOneOfTwoClaimsOnTheSameDueTimeWins(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, at(9, 8, 0))
	due, now := time.UnixMilli(a.NextRunAt), at(9, 9, 0)
	var wins int
	var mu sync.Mutex
	var wg sync.WaitGroup
	for range 8 {
		wg.Go(func() {
			if _, ok := s.Claim(a.ID, due, now); ok {
				mu.Lock()
				wins++
				mu.Unlock()
			}
		})
	}
	wg.Wait()
	_, runs := s.Snapshot()
	if wins != 1 || len(runs) != 1 || runs[0].Status != "pending" || runs[0].Trigger != "schedule" {
		t.Fatalf("%d wins, runs %+v", wins, runs)
	}
	if got, _ := s.Automation(a.ID); got.NextRunAt != at(12, 9, 0).UnixMilli() {
		t.Fatalf("next run %v", time.UnixMilli(got.NextRunAt))
	}
}

func TestAClaimSurvivesARestartSoTheSameDueTimeIsNotClaimedAgain(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	a := saved(t, s, at(9, 8, 0))
	due, now := time.UnixMilli(a.NextRunAt), at(9, 9, 0)
	if _, ok := s.Claim(a.ID, due, now); !ok {
		t.Fatal("first claim lost")
	}
	if _, ok := open(t, db).Claim(a.ID, due, now); ok {
		t.Fatal("claimed twice across a restart")
	}
}

func TestAClaimOnAPausedOrDeletedAutomationLoses(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, at(9, 8, 0))
	s.Enable(a.ID, false, at(9, 8, 1))
	if _, ok := s.Claim(a.ID, time.UnixMilli(a.NextRunAt), at(9, 9, 0)); ok {
		t.Fatal("claimed a paused automation")
	}
	if _, ok := s.Claim("nope", time.UnixMilli(a.NextRunAt), at(9, 9, 0)); ok {
		t.Fatal("claimed an unknown automation")
	}
}

func TestAClaimLaterThanTheGraceWindowIsSkipped(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, at(9, 8, 0))
	due := time.UnixMilli(a.NextRunAt)
	now := due.Add(Grace + time.Minute)
	run, ok := s.Claim(a.ID, due, now)
	if !ok || run.Status != "skipped" || run.Why != "Mac asleep" || run.StartedAt != due.UnixMilli() {
		t.Fatalf("%v %+v", ok, run)
	}
	if got, _ := s.Automation(a.ID); got.NextRunAt <= now.UnixMilli() {
		t.Fatalf("next run %v is not after now", time.UnixMilli(got.NextRunAt))
	}
}

func TestAClaimWhileARunIsActiveIsSkipped(t *testing.T) {
	for _, status := range []string{"pending", "running", "waiting"} {
		s := openNew(t)
		a := saved(t, s, at(9, 8, 0))
		first, _ := s.Begin(a.ID, at(9, 8, 30))
		s.Update(first.ID, func(r *proto.Run) { r.Status = status })
		run, ok := s.Claim(a.ID, time.UnixMilli(a.NextRunAt), at(9, 9, 0))
		if !ok || run.Status != "skipped" || run.Why != "Still working" {
			t.Fatalf("%s: %v %+v", status, ok, run)
		}
	}
}

func TestAClaimAfterTheLastRunFinishedStarts(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, at(9, 8, 0))
	first, _ := s.Begin(a.ID, at(9, 8, 30))
	s.Update(first.ID, func(r *proto.Run) { r.Status = "succeeded" })
	if run, ok := s.Claim(a.ID, time.UnixMilli(a.NextRunAt), at(9, 9, 0)); !ok || run.Status != "pending" {
		t.Fatalf("%v %+v", ok, run)
	}
}

func TestHistoryKeepsTheNewestFiftyRunsPerAutomation(t *testing.T) {
	s := openNew(t)
	a, b := saved(t, s, at(9, 8, 0)), saved(t, s, at(9, 8, 0))
	var first proto.Run
	for i := range history + 5 {
		run, err := s.Begin(a.ID, at(9, 8, 0).Add(time.Duration(i)*time.Second))
		if err != nil {
			t.Fatal(err)
		}
		if i == 0 {
			first = run
		}
		s.Update(run.ID, func(r *proto.Run) { r.Status = "succeeded" })
	}
	s.Begin(b.ID, at(9, 8, 0))
	_, runs := s.Snapshot()
	count := map[string]int{}
	for _, r := range runs {
		count[r.AutomationID]++
	}
	if count[a.ID] != history || count[b.ID] != 1 {
		t.Fatalf("counts %v", count)
	}
	if _, ok := s.Run(first.ID); ok {
		t.Fatal("the oldest run is still kept")
	}
}

func TestAShorterGraceSkipsARunTheDefaultWouldStart(t *testing.T) {
	s := openNew(t)
	s.Grace = func() time.Duration { return time.Hour }
	a := saved(t, s, at(9, 8, 0))
	due := time.UnixMilli(a.NextRunAt)
	if run, ok := s.Claim(a.ID, due, due.Add(2*time.Hour)); !ok || run.Status != "skipped" {
		t.Fatalf("%v %+v", ok, run)
	}
}

func TestHistoryKeepsAsManyRunsAsTheSettingSays(t *testing.T) {
	s := openNew(t)
	s.History = func() int { return 10 }
	a := saved(t, s, at(9, 8, 0))
	for i := range 12 {
		run, _ := s.Begin(a.ID, at(9, 8, 0).Add(time.Duration(i)*time.Second))
		s.Update(run.ID, func(r *proto.Run) { r.Status = "succeeded" })
	}
	if _, runs := s.Snapshot(); len(runs) != 10 {
		t.Fatalf("kept %d", len(runs))
	}
}

func TestSnapshotListsRunsNewestFirstAcrossAutomations(t *testing.T) {
	s := openNew(t)
	a, b := saved(t, s, at(9, 8, 0)), saved(t, s, at(9, 8, 0))
	r1, _ := s.Begin(a.ID, at(9, 8, 1))
	r2, _ := s.Begin(b.ID, at(9, 8, 2))
	skipped, _ := s.Claim(a.ID, time.UnixMilli(a.NextRunAt), at(9, 9, 0).Add(Grace+time.Hour))
	_, runs := s.Snapshot()
	var ids []string
	for _, r := range runs {
		ids = append(ids, r.ID)
	}
	// The skipped run's start is its due time, 09:00.
	if want := []string{skipped.ID, r2.ID, r1.ID}; !slices.Equal(ids, want) {
		t.Fatalf("got %v, want %v", ids, want)
	}
}

func TestDeletingAnAutomationDropsItsRuns(t *testing.T) {
	s := openNew(t)
	a, b := saved(t, s, at(9, 8, 0)), saved(t, s, at(9, 8, 0))
	s.Begin(a.ID, at(9, 8, 1))
	kept, _ := s.Begin(b.ID, at(9, 8, 1))
	if err := s.Delete(a.ID); err != nil {
		t.Fatal(err)
	}
	autos, runs := s.Snapshot()
	if len(autos) != 1 || autos[0].ID != b.ID || len(runs) != 1 || runs[0].ID != kept.ID {
		t.Fatalf("%+v %+v", autos, runs)
	}
	if s.Delete(a.ID) != ErrUnknown {
		t.Fatal("deleted twice")
	}
}

func TestRunNowIsRefusedWhileARunIsActive(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, at(9, 8, 0))
	run, err := s.Begin(a.ID, at(9, 8, 1))
	if err != nil || run.Trigger != "manual" || run.Status != "pending" {
		t.Fatalf("%+v %v", run, err)
	}
	if _, err := s.Begin(a.ID, at(9, 8, 2)); err != ErrBusy {
		t.Fatalf("got %v", err)
	}
	s.Update(run.ID, func(r *proto.Run) { r.Status = "failed" })
	if _, err := s.Begin(a.ID, at(9, 8, 3)); err != nil {
		t.Fatalf("got %v", err)
	}
	if _, err := s.Begin("nope", at(9, 8, 3)); err != ErrUnknown {
		t.Fatalf("got %v", err)
	}
}

func TestRunNowLeavesTheNextScheduledRunAlone(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, at(9, 8, 0))
	s.Begin(a.ID, at(9, 8, 1))
	if got, _ := s.Automation(a.ID); got.NextRunAt != a.NextRunAt {
		t.Fatalf("%v", time.UnixMilli(got.NextRunAt))
	}
}

func TestReconcileFailsRunsLeftPending(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	a := saved(t, s, at(9, 8, 0))
	pending, _ := s.Begin(a.ID, at(9, 8, 1))
	other, _ := s.Begin(saved(t, s, at(9, 8, 0)).ID, at(9, 8, 1))
	s.Update(other.ID, func(r *proto.Run) { r.Status = "running" })
	s = open(t, db)
	if err := s.Reconcile(at(9, 8, 5)); err != nil {
		t.Fatal(err)
	}
	got, _ := s.Run(pending.ID)
	if got.Status != "failed" || got.Why != "Interrupted" || got.FinishedAt != at(9, 8, 5).UnixMilli() {
		t.Fatalf("%+v", got)
	}
	if got, _ := s.Run(other.ID); got.Status != "running" {
		t.Fatalf("a running run became %q", got.Status)
	}
}

func TestEverySaveClosesTheChangedChannel(t *testing.T) {
	s := openNew(t)
	changed := s.Changed()
	select {
	case <-changed:
		t.Fatal("closed before a save")
	default:
	}
	a := saved(t, s, at(9, 8, 0))
	select {
	case <-changed:
	default:
		t.Fatal("not closed by a save")
	}
	next := s.Changed()
	s.Enable(a.ID, true, at(9, 8, 1))
	select {
	case <-next:
		t.Fatal("closed by a change that changed nothing")
	default:
	}
}

func TestAFailedSaveLeavesTheStoreAsItWas(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	a := saved(t, s, at(9, 8, 0))
	elsewhere := morning
	elsewhere.Folder = "/nowhere"
	if _, err := s.Save(elsewhere, at(9, 8, 0)); !errors.Is(err, registry.ErrUnknownProject) {
		t.Fatalf("got %v", err)
	}
	moved := a
	moved.Folder = "/nowhere"
	if _, err := s.Save(moved, at(9, 8, 0)); !errors.Is(err, registry.ErrUnknownProject) {
		t.Fatalf("moving it: %v", err)
	}
	for _, store := range []*Store{s, open(t, db)} {
		if autos, _ := store.Snapshot(); len(autos) != 1 || !reflect.DeepEqual(autos[0], a) {
			t.Fatalf("%+v", autos)
		}
	}
}

func TestAWebhookOnlyAutomationNeverRunsOnATimer(t *testing.T) {
	s := openNew(t)
	hook := morning
	hook.Schedule, hook.Webhook = proto.Schedule{Kind: "webhook"}, true
	a, err := s.Save(hook, at(9, 8, 0))
	if err != nil || a.NextRunAt != 0 {
		t.Fatalf("webhook only: next run %d, %v", a.NextRunAt, err)
	}
	hook.Schedule = proto.Schedule{}
	if _, err := s.Save(hook, at(9, 8, 0)); err != ErrInvalid {
		t.Fatalf("a webhook with no schedule kind: %v", err)
	}
	hook.Schedule, hook.Webhook = proto.Schedule{Kind: "webhook"}, false
	if _, err := s.Save(hook, at(9, 8, 0)); err != ErrInvalid {
		t.Fatalf("the webhook schedule without a webhook: %v", err)
	}
}

func TestAWebhookSecretSurvivesARestartButNotOnTheWire(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	hook := morning
	hook.Schedule, hook.Webhook, hook.WebhookSecret = proto.Schedule{Kind: "webhook"}, true, "s3cret"
	a, err := s.Save(hook, at(9, 8, 0))
	if err != nil {
		t.Fatal(err)
	}
	reopened, ok := open(t, db).Automation(a.ID)
	if !ok || reopened.WebhookSecret != "s3cret" {
		t.Fatalf("the secret did not survive: %+v", reopened)
	}
	raw, _ := json.Marshal(proto.NewAutomations([]proto.Automation{reopened}, nil))
	if strings.Contains(string(raw), "s3cret") {
		t.Fatalf("the secret is on the wire: %s", raw)
	}
}

func TestAnAutomationInAWorktreeKeepsItsFolderAcrossARestart(t *testing.T) {
	home := t.TempDir()
	repo, tree := filepath.Join(home, "repo"), filepath.Join(home, "repo-wt")
	gitdir := filepath.Join(repo, ".git", "worktrees", "wt")
	for _, dir := range []string{gitdir, tree} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(gitdir, "commondir"), []byte("../.."), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tree, ".git"), []byte("gitdir: "+gitdir), 0o644); err != nil {
		t.Fatal(err)
	}
	db := dbtest.Open(t, filepath.Join(home, "pocketd.db"))
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	if err := reg.AddProject(repo, nil); err != nil {
		t.Fatal(err)
	}
	s, err := Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	inTree, inRepo := morning, morning
	inTree.Folder, inRepo.Folder = tree, repo
	for _, a := range []proto.Automation{inTree, inRepo} {
		if _, err := s.Save(a, at(9, 8, 0)); err != nil {
			t.Fatal(err)
		}
	}
	reopened, err := Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	autos, _ := reopened.Snapshot()
	if len(autos) != 2 || autos[0].Folder != tree || autos[1].Folder != repo {
		t.Fatalf("got %+v", autos)
	}
}

func TestEveryAutomationAndRunSurvivesARestartInOrder(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	s.History = func() int { return 2 }
	first := saved(t, s, at(9, 8, 0))
	other := morning
	other.Folder, other.Bound = "/other", "ag1"
	second, err := s.Save(other, at(9, 8, 0))
	if err != nil {
		t.Fatal(err)
	}
	for i := range 3 {
		run, err := s.Begin(first.ID, at(9, 8, 1+i))
		if err != nil {
			t.Fatal(err)
		}
		s.Update(run.ID, func(r *proto.Run) { r.Status, r.TurnAfter, r.TurnPrompt, r.TurnEntry = "succeeded", 7, "p", "q_1" })
	}
	if _, err := s.Begin(second.ID, at(9, 8, 9)); err != nil {
		t.Fatal(err)
	}
	wantAutos, wantRuns := s.Snapshot()
	gotAutos, gotRuns := open(t, db).Snapshot()
	if len(wantRuns) != 3 || !reflect.DeepEqual(gotAutos, wantAutos) || !reflect.DeepEqual(gotRuns, wantRuns) {
		t.Fatalf("got %+v %+v\nwant %+v %+v", gotAutos, gotRuns, wantAutos, wantRuns)
	}
}

func TestDeletingAnAutomationDeletesItsRunsAndNoOthers(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	a, b := saved(t, s, at(9, 8, 0)), saved(t, s, at(9, 8, 0))
	s.Begin(a.ID, at(9, 8, 1))
	keep, _ := s.Begin(b.ID, at(9, 8, 1))
	if err := s.Delete(a.ID); err != nil {
		t.Fatal(err)
	}
	autos, runs := open(t, db).Snapshot()
	if len(autos) != 1 || autos[0].ID != b.ID || len(runs) != 1 || runs[0].ID != keep.ID {
		t.Fatalf("got %+v %+v", autos, runs)
	}
}

func TestRemovingAProjectDropsItsAutomationsAndRunsAndWakesWhoWatches(t *testing.T) {
	db := newDB(t)
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	s, err := Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	reg.OnRemove(func(string) { s.Reload() })
	a := saved(t, s, at(9, 8, 0))
	elsewhere := morning
	elsewhere.Folder = "/other"
	b, err := s.Save(elsewhere, at(9, 8, 0))
	if err != nil {
		t.Fatal(err)
	}
	s.Begin(a.ID, at(9, 8, 1))
	keep, _ := s.Begin(b.ID, at(9, 8, 1))
	changed := s.Changed()
	if err := reg.RemoveProject("/f"); err != nil {
		t.Fatal(err)
	}
	select {
	case <-changed:
	default:
		t.Fatal("watchers weren't told")
	}
	autos, runs := s.Snapshot()
	if len(autos) != 1 || autos[0].ID != b.ID || len(runs) != 1 || runs[0].ID != keep.ID {
		t.Fatalf("got %+v %+v", autos, runs)
	}
	var n int
	db.QueryRow(`SELECT (SELECT COUNT(*) FROM automations) + (SELECT COUNT(*) FROM runs)`).Scan(&n)
	if n != 2 {
		t.Fatalf("%d rows", n)
	}
}
