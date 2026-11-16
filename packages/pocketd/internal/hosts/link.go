package hosts

import (
	"math/rand/v2"
	"time"
)

type LinkState string

const (
	StateConnecting LinkState = "connecting"
	StateOnline     LinkState = "online"
	// StateRetrying tries again by itself.
	StateRetrying LinkState = "retrying"
	// StateFailed waits for a retry.
	StateFailed LinkState = "failed"
)

// Link is a host's connection.
type Link struct {
	State   LinkState
	Version string
	Uptime  uint64
	Failure Failure
}

// Backoff counts failures in a row; a link that reached pocketd starts over, so a flapping host retries quickly.
type Backoff struct{ attempt int }

func (b *Backoff) Reset() { b.attempt = 0 }

// Fail is the link to show after failure, and how long to wait before trying again; false waits for the user.
func (b *Backoff) Fail(failure Failure) (Link, time.Duration, bool) {
	wait, retry := failure.retryAfter(b.attempt)
	b.attempt++
	if retry {
		return Link{State: StateRetrying, Failure: failure}, wait, true
	}
	return Link{State: StateFailed, Failure: failure}, 0, false
}

// jitter shortens a wait by up to half so hosts that dropped together retry apart, but lengthens a throttled one: early retries extend sshd's penalty.
func jitter(wait time.Duration, failure Failure) time.Duration {
	random := float64(rand.IntN(1000)) / 2000
	if failure == FailThrottled {
		return time.Duration(float64(wait) * (1 + random))
	}
	return time.Duration(float64(wait) * (1 - random))
}
