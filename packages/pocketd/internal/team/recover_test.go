package team

import (
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

func TestARestartLeavesEachTeamAsWhatItsAgentsAndCheckoutSay(t *testing.T) {
	asking, _ := WorkerStatus(t1Working(t), "w1", "needsYou")
	turnFailed, _ := Failed(t1Working(t), "t1", "The worker's turn failed")
	gone, _ := Failed(t1Working(t), "t1", WorkerRestarted)
	launching := Starting(startedPlan(t), "t1", nil)
	launchingGone, _ := Failed(launching, "t1", WorkerRestarted)
	planning := proto.Team{Lead: testLead, State: "planning"}
	handed := t1Working(t)
	handed.Tasks[0].Turn = "u1"
	merging, _ := Merging(reviewedPair())
	aborted := Conflicted(merging, nil)
	aborted.Error = MergeRestarted
	for name, c := range map[string]struct {
		team    proto.Team
		facts   Facts
		want    proto.Team
		r       Recovery
		changed bool
	}{
		"a worker at work shows its status":               {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "needsYou"}}}, asking, Recovery{}, true},
		"a worker still on its turn changes nothing":      {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "working"}}}, t1Working(t), Recovery{}, false},
		"a worker that finished hands back":               {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "done", Turn: "u1", Ended: true, OK: true}}}, t1Working(t), Recovery{HandBack: []string{"t1"}}, false},
		"a turn already handed back isn't again":          {handed, Facts{Workers: map[string]Worker{"t1": {Status: "done", Turn: "u1", Ended: true, OK: true}}}, handed, Recovery{}, false},
		"the worker's next turn hands back":               {handed, Facts{Workers: map[string]Worker{"t1": {Status: "done", Turn: "u2", Ended: true, OK: true}}}, handed, Recovery{HandBack: []string{"t1"}}, false},
		"a worker whose turn failed fails":                {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "done", Ended: true}}}, turnFailed, Recovery{Failed: []string{"t1"}}, true},
		"a worker that is gone fails":                     {t1Working(t), Facts{Workers: map[string]Worker{"t1": {}}}, gone, Recovery{Failed: []string{"t1"}}, true},
		"a worker back without its turn's end fails":      {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "idle"}}}, gone, Recovery{Failed: []string{"t1"}}, true},
		"a task still launching fails":                    {launching, Facts{}, launchingGone, Recovery{Failed: []string{"t1"}}, true},
		"a planning lead that is gone fails the team":     {planning, Facts{}, proto.Team{Lead: testLead, State: "failed", Error: LeadRestarted}, Recovery{}, true},
		"a planning lead back and idle is read again":     {planning, Facts{Lead: "idle"}, planning, Recovery{Reread: true}, false},
		"a planning lead still working is left to finish": {planning, Facts{Lead: "working"}, planning, Recovery{}, false},
		"a merge in progress is aborted as a conflict":    {merging, Facts{Merging: true}, aborted, Recovery{Abort: true}, true},
		"a merge not in progress goes back to review":     {merging, Facts{}, MergeFailed(merging, MergeRestarted), Recovery{}, true},
		"a proposed plan is left as it is":                {planOfThree(), Facts{}, planOfThree(), Recovery{}, false},
	} {
		got, r, changed := Recovered(c.team, c.facts)
		if !reflect.DeepEqual(got, c.want) || !reflect.DeepEqual(r, c.r) || changed != c.changed {
			t.Errorf("%s: got %t %+v\n%+v", name, changed, r, got)
		}
	}
}
