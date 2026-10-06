package launch

import (
	"crypto/sha256"
	"encoding/json"
	"sync"
	"time"

	"pocketd/internal/proto"
)

const receiptTTL = 10 * time.Minute

type receipt struct {
	hash     [32]byte
	done     chan struct{}
	creating Creating
	result   Result
	at       time.Time
}

type receipts struct {
	mu  sync.Mutex
	now func() time.Time
	m   map[string]*receipt
}

func newReceipts(now func() time.Time) *receipts {
	return &receipts{now: now, m: map[string]*receipt{}}
}

// take returns key's receipt and whether the caller must run the create.
// A nil receipt means key already holds a different spec.
func (r *receipts) take(key string, s proto.LaunchSpec) (*receipt, bool) {
	raw, _ := json.Marshal(s)
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
	x := &receipt{hash: hash, done: make(chan struct{})}
	r.m[key] = x
	return x, true
}

func (r *receipts) creating(x *receipt, c Creating) {
	r.mu.Lock()
	x.creating = c
	r.mu.Unlock()
}

func (r *receipts) finish(x *receipt, res Result) {
	r.mu.Lock()
	x.result, x.at = res, r.now()
	r.mu.Unlock()
	close(x.done)
}

func (r *receipts) outcome(x *receipt) (Creating, Result) {
	r.mu.Lock()
	defer r.mu.Unlock()
	return x.creating, x.result
}

// inFlight reports whether a create hasn't finished.
func (r *receipts) inFlight() bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, x := range r.m {
		if x.at.IsZero() {
			return true
		}
	}
	return false
}
