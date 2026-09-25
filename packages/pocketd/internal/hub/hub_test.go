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
