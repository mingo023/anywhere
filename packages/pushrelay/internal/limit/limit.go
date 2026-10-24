// Package limit keeps token buckets in memory, one per key.
package limit

import (
	"sync"
	"time"
)

// Rate refills PerHour tokens an hour into a bucket holding at most Burst.
type Rate struct {
	PerHour float64
	Burst   float64
}

type bucket struct {
	tokens float64
	at     time.Time
}

// Limiter is a set of buckets of one Rate. Buckets full again are forgotten.
type Limiter struct {
	rate Rate
	mu   sync.Mutex
	b    map[string]*bucket
	seen int
}

func New(r Rate) *Limiter { return &Limiter{rate: r, b: map[string]*bucket{}} }

// Take spends one token of key's bucket. When it is empty, ok is false and
// resetsAt is when the next token arrives.
func (l *Limiter) Take(key string, now time.Time) (ok bool, resetsAt time.Time) {
	l.mu.Lock()
	defer l.mu.Unlock()
	perSec := l.rate.PerHour / 3600
	if l.seen++; l.seen%1024 == 0 {
		l.sweep(now, perSec)
	}
	b := l.b[key]
	if b == nil {
		b = &bucket{tokens: l.rate.Burst, at: now}
		l.b[key] = b
	}
	b.tokens = min(l.rate.Burst, b.tokens+now.Sub(b.at).Seconds()*perSec)
	b.at = now
	if b.tokens < 1 {
		return false, now.Add(time.Duration((1 - b.tokens) / perSec * float64(time.Second)))
	}
	b.tokens--
	return true, time.Time{}
}

// sweep forgets buckets that refilled, so memory follows the keys in use.
func (l *Limiter) sweep(now time.Time, perSec float64) {
	for k, b := range l.b {
		if b.tokens+now.Sub(b.at).Seconds()*perSec >= l.rate.Burst {
			delete(l.b, k)
		}
	}
}
