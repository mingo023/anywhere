package wsserver

import (
	"testing"

	"pocketd/internal/peer"
)

func TestATokenCannotCallAWebSocketVerb(t *testing.T) {
	_, d, p := local(t, peer.Delegated("t1"))
	helloAs(p)
	for _, raw := range []string{
		`{"type":"agent.list","id":"l"}`,
		`{"type":"agent.prompt","id":"p","agentId":"a1","text":"hi"}`,
		`{"type":"permission.resolve","id":"r","requestId":"x","decision":"allow"}`,
	} {
		p.send(raw)
		if m := p.recv(); m["code"] != "scope_denied" {
			t.Fatalf("%s: %v", raw, m)
		}
	}
	if len(d.prompts) != 0 {
		t.Fatalf("typed %v", d.prompts)
	}
}
