package push

import (
	"bytes"
	"fmt"
	"testing"

	"pocketd/internal/proto"
)

// Build runs on every status change for every registered phone.
func BenchmarkBuild1000Agents(b *testing.B) {
	statuses := []string{"working", "needsYou", "done", "idle"}
	agents := make([]proto.AgentSummary, 1000)
	for i := range agents {
		agents[i] = agentAt(fmt.Sprint("a", i), statuses[i%len(statuses)], int64(i))
	}
	for b.Loop() {
		Build(agents, nil, true)
	}
}

func BenchmarkSealANotice(b *testing.B) {
	key := bytes.Repeat([]byte{7}, 32)
	plain := []byte(`{"mac":"mingos-mbp","agent":"a1","kind":"needsYou","title":"Approve the migration","place":"anywhere · calm-otter","line":"Wants to run rm -rf build","request":"r1","badge":2}`)
	for b.Loop() {
		if _, err := Seal(key, "mingos-mbp", plain); err != nil {
			b.Fatal(err)
		}
	}
}
