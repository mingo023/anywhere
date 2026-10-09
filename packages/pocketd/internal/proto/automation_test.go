package proto

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

func golden(t *testing.T, side, name string) []byte {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("testdata", "golden", side, name))
	if err != nil {
		t.Fatal(err)
	}
	return bytes.TrimSpace(raw)
}

func TestTheSnapshotGoldenIsWhatTheServerWrites(t *testing.T) {
	raw := golden(t, "server", "automations.json")
	var snap Automations
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&snap); err != nil {
		t.Fatal(err)
	}
	got, _ := json.Marshal(snap)
	if !bytes.Equal(got, raw) {
		t.Fatalf("got  %s\nwant %s", got, raw)
	}
}

func TestAnEmptySnapshotHasEmptyLists(t *testing.T) {
	got, _ := json.Marshal(NewAutomations(nil, nil))
	if string(got) != `{"type":"automations","automations":[],"runs":[]}` {
		t.Fatalf("%s", got)
	}
}

func TestTheAutomationGoldensDecodeToTheirFields(t *testing.T) {
	m, err := DecodeClient(golden(t, "client", "automation_save.json"))
	if err != nil || m.Automation == nil || m.Automation.ID != "au1" || m.Automation.Schedule.Time != "09:00" || !slices.Equal(m.Automation.Schedule.Days, []int{1, 2, 3, 4, 5}) {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient(golden(t, "client", "automation_save_interval.json"))
	if err != nil || m.Automation.ID != "" || m.Automation.Provider != "codex" || m.Automation.Schedule.EveryMin != 60 {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient(golden(t, "client", "automation_enable.json"))
	if err != nil || m.AutomationID != "au1" || m.Enabled {
		t.Fatalf("%+v %v", m, err)
	}
	for _, name := range []string{"automation_delete.json", "automation_run.json"} {
		if m, err = DecodeClient(golden(t, "client", name)); err != nil || m.AutomationID != "au1" {
			t.Fatalf("%s: %+v %v", name, m, err)
		}
	}
}

func TestAnAutomationSaveWithAnythingOffTheContractIsMalformed(t *testing.T) {
	const days = `{"kind":"days","days":[1],"time":"09:00"}`
	save := func(automation string) string {
		return `{"type":"automation.save","id":"1","automation":` + automation + `}`
	}
	good := func(fields string) string {
		return `{"name":"n","prompt":"p","provider":"claude","folder":"/f","schedule":` + days + `,"enabled":true` + fields + `}`
	}
	withSchedule := func(s string) string {
		return strings.Replace(good(""), days, s, 1)
	}
	for _, raw := range []string{
		save(good(`,"nextRunAt":1`)),
		save(good(`,"Name":"x"`)),
		save(good(`,"id":null`)),
		save(strings.Replace(good(""), `"name":"n",`, "", 1)),
		save(strings.Replace(good(""), `,"enabled":true`, "", 1)),
		save(strings.Replace(good(""), `"name":"n"`, `"name":"  "`, 1)),
		save(strings.Replace(good(""), `"name":"n"`, `"name":"`+strings.Repeat("x", MaxName+1)+`"`, 1)),
		save(strings.Replace(good(""), `"prompt":"p"`, `"prompt":""`, 1)),
		save(strings.Replace(good(""), `"prompt":"p"`, `"prompt":"`+strings.Repeat("x", MaxPrompt+1)+`"`, 1)),
		save(strings.Replace(good(""), `"claude"`, `"gemini"`, 1)),
		save(strings.Replace(good(""), `"folder":"/f"`, `"folder":""`, 1)),
		save(strings.Replace(good(""), `"enabled":true`, `"enabled":"yes"`, 1)),
		save(withSchedule(`{"kind":"days","days":[],"time":"09:00"}`)),
		save(withSchedule(`{"kind":"days","days":[7],"time":"09:00"}`)),
		save(withSchedule(`{"kind":"days","days":[1,1],"time":"09:00"}`)),
		save(withSchedule(`{"kind":"days","days":[1.5],"time":"09:00"}`)),
		save(withSchedule(`{"kind":"days","days":[1],"time":"9:00"}`)),
		save(withSchedule(`{"kind":"days","days":[1],"time":"24:00"}`)),
		save(withSchedule(`{"kind":"days","days":[1],"time":"09:60"}`)),
		save(withSchedule(`{"kind":"days","days":[1],"time":"09:00","everyMin":60}`)),
		save(withSchedule(`{"kind":"days","days":[1],"time":"09:00","Kind":"x"}`)),
		save(withSchedule(`{"kind":"interval","everyMin":4}`)),
		save(withSchedule(`{"kind":"interval","everyMin":10081}`)),
		save(withSchedule(`{"kind":"interval","everyMin":60,"days":[1]}`)),
		save(withSchedule(`{"kind":"interval","everyMin":60,"time":"09:00"}`)),
		save(withSchedule(`{"kind":"weekly"}`)),
		save(good(`,"access":"plan"`)),
		save(good(`,"newWorktree":"yes"`)),
		save(`null`),
		`{"type":"automation.save","id":"1"}`,
		`{"type":"automation.enable","id":"1","automationId":"a"}`,
		`{"type":"automation.enable","id":"1","enabled":true}`,
		`{"type":"automation.delete","id":"1"}`,
		`{"type":"automation.run","id":"1","automationId":null}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", raw, err)
		}
	}
}

func TestAnAutomationSaveCarriesItsAccessAndWorktreeAndOlderOnesGoWithout(t *testing.T) {
	const old = `{"type":"automation.save","id":"1","automation":{"name":"n","prompt":"p","provider":"claude","folder":"/f","schedule":{"kind":"interval","everyMin":60},"enabled":true}}`
	m, err := DecodeClient([]byte(strings.Replace(old, `"enabled":true`, `"enabled":true,"access":"edits","newWorktree":true`, 1)))
	if err != nil || m.Automation.Access != "edits" || !m.Automation.NewWorktree {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient([]byte(old))
	if err != nil || m.Automation.Access != "" || m.Automation.NewWorktree {
		t.Fatalf("%+v %v", m, err)
	}
}

func TestInvalidSchedulesAreRefused(t *testing.T) {
	for name, s := range map[string]Schedule{
		"no kind":             {},
		"days without days":   {Kind: "days", Time: "09:00"},
		"days without a time": {Kind: "days", Days: []int{1}},
		"interval too short":  {Kind: "interval", EveryMin: MinEveryMin - 1},
		"interval too long":   {Kind: "interval", EveryMin: MaxEveryMin + 1},
		"mixed":               {Kind: "interval", EveryMin: 60, Days: []int{1}},
	} {
		if s.Valid() {
			t.Errorf("%s is valid", name)
		}
	}
	for name, s := range map[string]Schedule{
		"every day":   {Kind: "days", Days: []int{0, 1, 2, 3, 4, 5, 6}, Time: "00:00"},
		"shortest":    {Kind: "interval", EveryMin: MinEveryMin},
		"a full week": {Kind: "interval", EveryMin: MaxEveryMin},
	} {
		if !s.Valid() {
			t.Errorf("%s is invalid", name)
		}
	}
}
