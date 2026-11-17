package launch

import (
	"testing"
	"time"

	"pocketd/internal/proto"
)

func TestADuplicateReturnsTheFirstResult(t *testing.T) {
	r := newReceipts[built](time.Now)
	n := NewWorktree{Project: "/p", Name: "calm-otter"}
	x, fresh := r.take("owner\x00r1", n)
	if !fresh {
		t.Fatal("first take is not fresh")
	}
	r.finish(x, built{made: Made{Path: "/wt/calm-otter"}})
	y, fresh := r.take("owner\x00r1", n)
	if fresh || y != x {
		t.Fatal("a retry started a second create")
	}
	<-y.done
	if b := r.outcome(y); b.made.Path != "/wt/calm-otter" {
		t.Fatalf("got %+v", b)
	}
}

func TestADifferentSpecUnderTheSameRequestIsADuplicate(t *testing.T) {
	r := newReceipts[built](time.Now)
	r.take("owner\x00r1", spec("claude", "ask", false))
	if x, _ := r.take("owner\x00r1", spec("claude", "full", false)); x != nil {
		t.Fatal("a different spec joined the first create")
	}
	if _, fresh := r.take("device:d1\x00r1", spec("claude", "full", false)); !fresh {
		t.Fatal("another principal's request id collided")
	}
}

func TestAReceiptExpiresAfterTenMinutes(t *testing.T) {
	now := time.Unix(0, 0)
	r := newReceipts[built](func() time.Time { return now })
	s := proto.LaunchSpec{Provider: "claude"}
	x, _ := r.take("k", s)
	now = now.Add(time.Hour)
	if _, fresh := r.take("k", s); fresh {
		t.Fatal("an in-flight receipt expired")
	}
	r.finish(x, built{})
	now = now.Add(10*time.Minute + time.Second)
	if _, fresh := r.take("k", s); !fresh {
		t.Fatal("a finished receipt outlived ten minutes")
	}
}
