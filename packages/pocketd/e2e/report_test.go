//go:build unix

package e2e

import "testing"

func TestAClaudeThatStopsReportingStillEndsItsTurnOnTheStopHook(t *testing.T) {
	h, phone, term, id := StartClaude(t)
	h.Prompt(term, "quiet")
	phone.WaitFor("the working report", func(m Message) bool {
		return statusOf(id, "working")(m) && m.Agent.Activity == "Reading daemon.go"
	})
	phone.WaitStatus(id, "done")
}
