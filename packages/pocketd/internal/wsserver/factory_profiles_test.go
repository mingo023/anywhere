package wsserver

import (
	"testing"

	"pocketd/internal/proto"
)

func TestOnlyAnOwnerWithTheFactoryMaySendAFrameAsLargeAsAProfile(t *testing.T) {
	if got := readLimit(true, []string{proto.CapFactory}); got != factoryReadLimit || got < 3<<20 {
		t.Fatalf("an owner with the Factory reads %d", got)
	}
	if readLimit(false, []string{proto.CapFactory}) != authedReadLimit || readLimit(true, []string{proto.CapBoard}) != authedReadLimit {
		t.Fatal("a phone, or an owner without the Factory, got the large limit")
	}
}
