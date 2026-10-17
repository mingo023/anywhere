package hub

import "testing"

func TestPublishReachesEverySubscriber(t *testing.T) {
	h := New()
	a, _ := h.Subscribe()
	b, _ := h.Subscribe()
	h.Publish(map[string]int{"n": 1})
	for _, ch := range []<-chan []byte{a, b} {
		if got := string(<-ch); got != `{"n":1}` {
			t.Fatal(got)
		}
	}
}

func TestSlowSubscriberIsDropped(t *testing.T) {
	h := New()
	slow, _ := h.Subscribe()
	for range buffer + 1 {
		h.Publish(1)
	}
	n := 0
	for range slow {
		n++
	}
	if n != buffer {
		t.Fatalf("got %d then close, want %d", n, buffer)
	}
}

func TestUnsubscribeTwiceIsSafe(t *testing.T) {
	h := New()
	_, stop := h.Subscribe()
	stop()
	stop()
	h.Publish(1)
}

func TestASubagentsItemReachesOnlySubscribersThatTakeThem(t *testing.T) {
	h := New()
	plain, _ := h.Subscribe()
	subs, _ := h.SubscribeSubagents()
	h.PublishSubagent(1)
	h.Publish(2)
	if got := string(<-subs); got != "1" {
		t.Fatal(got)
	}
	if got := string(<-plain); got != "2" {
		t.Fatalf("plain subscriber got %s first", got)
	}
}
