package launch

import (
	"crypto/sha256"
	"encoding/json"
	"sync"
	"time"
)

const receiptTTL = 10 * time.Minute

type receipt[T any] struct {
	hash     [32]byte
	done     chan struct{}
	creating Creating
	result   T
	at       time.Time
}

// receipts holds each create's outcome, T, under its requester's request id.
type receipts[T any] struct {
	mu  sync.Mutex
	now func() time.Time
	m   map[string]*receipt[T]
}

func newReceipts[T any](now func() time.Time) *receipts[T] {
	return &receipts[T]{now: now, m: map[string]*receipt[T]{}}
}

// take returns key's receipt and whether the caller must run the create.
// A nil receipt means key already holds a different spec.
func (r *receipts[T]) take(key string, spec any) (*receipt[T], bool) {
	raw, _ := json.Marshal(spec)
	hash := sha256.Sum256(raw)
	r.mu.Lock()
	defer r.mu.Unlock()
	now := r.now()
	for k, x := range r.m {
		if !x.at.IsZero() && now.Sub(x.at) > receiptTTL {
			delete(r.m, k)
		}
	}
	if x, ok := r.m[key]; ok {
		if x.hash != hash {
			return nil, false
		}
		return x, false
	}
	x := &receipt[T]{hash: hash, done: make(chan struct{})}
	r.m[key] = x
	return x, true
}

func (r *receipts[T]) creating(x *receipt[T], c Creating) {
	r.mu.Lock()
	x.creating = c
	r.mu.Unlock()
}

func (r *receipts[T]) finish(x *receipt[T], res T) {
	r.mu.Lock()
	x.result, x.at = res, r.now()
	r.mu.Unlock()
	close(x.done)
}

func (r *receipts[T]) outcome(x *receipt[T]) (Creating, T) {
	r.mu.Lock()
	defer r.mu.Unlock()
	return x.creating, x.result
}

// inFlight reports whether a create hasn't finished.
func (r *receipts[T]) inFlight() bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, x := range r.m {
		if x.at.IsZero() {
			return true
		}
	}
	return false
}
