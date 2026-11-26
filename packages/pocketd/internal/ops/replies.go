package ops

import (
	"container/list"
	"crypto/sha256"
	"encoding/json"
)

const maxReplies = 8192

var cacheable = map[string]bool{"list": true, "exec": true, "fs.list": true, "fs.stat": true, "fs.read": true}

type sentHashes struct {
	order *list.List
	at    map[[32]byte]*list.Element
}

type sentHash struct{ key, sum [32]byte }

func newSentHashes() *sentHashes {
	return &sentHashes{order: list.New(), at: map[[32]byte]*list.Element{}}
}

// repeats records sum as key's reply and reports whether it was already.
func (r *sentHashes) repeats(key, sum [32]byte) bool {
	if e, ok := r.at[key]; ok {
		r.order.MoveToBack(e)
		s := e.Value.(*sentHash)
		same := s.sum == sum
		s.sum = sum
		return same
	}
	if r.order.Len() >= maxReplies {
		delete(r.at, r.order.Remove(r.order.Front()).(*sentHash).key)
	}
	r.at[key] = r.order.PushBack(&sentHash{key, sum})
	return false
}

// requestKey is raw without its id and cached, so it stays apart from a request that differs only in a field pocketd ignores.
func requestKey(raw json.RawMessage) *[32]byte {
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil {
		return nil
	}
	delete(fields, "id")
	delete(fields, "cached")
	b, err := json.Marshal(fields)
	if err != nil {
		return nil
	}
	key := sha256.Sum256(b)
	return &key
}

// Reply sends r, or a bare Unchanged when req is Cached and r matches this conn's last reply to req.
// Errors skip the table and the client never holds one, since a refusal is sent without coming through here.
func (c *Conn) Reply(req, r Msg) error {
	if req.key == nil || r.Ev == "error" {
		return c.Send(r)
	}
	id := r.ID
	r.ID = ""
	b, err := json.Marshal(r)
	if err != nil {
		return err
	}
	sum := sha256.Sum256(b)
	r.ID = id
	// The table changes in the order the replies go out, which is the order the client reads them in.
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.sent.repeats(*req.key, sum) && req.Cached {
		return c.enc.Encode(Msg{Ev: r.Ev, ID: id, Unchanged: true})
	}
	return c.enc.Encode(r)
}
