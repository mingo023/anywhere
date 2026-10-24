package limit

import (
	"testing"
	"time"
)

func TestABucketLetsItsBurstThroughThenRefillsAtItsRate(t *testing.T) {
	l := New(Rate{PerHour: 120, Burst: 10})
	now := time.Unix(1000, 0)
	for i := range 10 {
		if ok, _ := l.Take("h", now); !ok {
			t.Fatalf("take %d refused", i)
		}
	}
	ok, resets := l.Take("h", now)
	if ok || resets != now.Add(30*time.Second) {
		t.Fatalf("%v %v", ok, resets)
	}
	if ok, _ := l.Take("h", now.Add(30*time.Second)); !ok {
		t.Fatal("no token after its refill")
	}
	if ok, _ := l.Take("other", now); !ok {
		t.Fatal("one key's bucket limited another")
	}
}

func TestBucketsThatRefilledAreForgotten(t *testing.T) {
	l := New(Rate{PerHour: 3600, Burst: 1})
	now := time.Unix(0, 0)
	for i := range 1023 {
		l.Take(string(rune('a'+i%26))+string(rune(i)), now)
	}
	l.Take("last", now.Add(time.Hour))
	if len(l.b) != 1 {
		t.Fatalf("%d buckets", len(l.b))
	}
}
