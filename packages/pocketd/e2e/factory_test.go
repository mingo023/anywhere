//go:build unix

package e2e

import (
	"testing"

	"pocketd/internal/proto"
)

func TestAServedPocketdTakesFactoryVerbs(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapFactory)
	o.Send(map[string]any{"type": "factory.config.get", "id": "g1", "project": "/elsewhere"})
	if e := o.WaitFor("the answer to g1", func(m Message) bool { return m.ID == "g1" }); e.Type != "error" || e.Code != proto.CodeUnknownProject {
		t.Fatalf("got %s", e.Raw)
	}
}
