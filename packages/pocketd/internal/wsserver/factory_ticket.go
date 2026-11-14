package wsserver

import (
	"context"
	"errors"
	"slices"
	"sync"

	"pocketd/internal/board"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

// factoryTicket answers with one Ticket's spec and activity.
func (c *conn) factoryTicket(m proto.ClientMessage) error {
	if c.s.Factory == nil || !slices.Contains(c.caps, proto.CapFactory) {
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryOff, "The Factory is off"))
		return nil
	}
	detail, err := c.s.Factory.Detail(m.TicketID)
	switch {
	case errors.Is(err, board.ErrUnknown):
		c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownTicket, err.Error()))
	case err != nil:
		c.send(proto.NewError(m.ID, err.Error()))
	default:
		c.send(proto.NewFactoryTicket(m.ID, detail))
	}
	return nil
}

// factoryUpdates tells the owner's desktop which Ticket's detail changed. A second hello keeps the first feed.
func (c *conn) factoryUpdates(caps []string) {
	if c.s.Factory == nil || c.stopFactory != nil || !c.who.Has(peer.Own) || !slices.Contains(caps, proto.CapFactory) {
		return
	}
	ids := c.s.factoryFeed.join(c.s.Factory.Changed())
	ctx, cancel := context.WithCancel(c.ctx)
	c.stopFactory = func() {
		cancel()
		c.s.factoryFeed.leave(ids)
	}
	go func() {
		for {
			select {
			case <-ctx.Done():
				return
			case id := <-ids:
				c.send(proto.NewFactoryUpdate(id))
			}
		}
	}()
}

// factoryFeed fans the engine's one channel out to every conn that joined.
type factoryFeed struct {
	once sync.Once
	mu   sync.Mutex
	subs map[chan string]struct{}
}

func (f *factoryFeed) join(changed <-chan string) chan string {
	f.once.Do(func() {
		go func() {
			for id := range changed {
				f.mu.Lock()
				for sub := range f.subs {
					// An update is a hint: a conn that is 64 behind misses one.
					select {
					case sub <- id:
					default:
					}
				}
				f.mu.Unlock()
			}
		}()
	})
	sub := make(chan string, 64)
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.subs == nil {
		f.subs = map[chan string]struct{}{}
	}
	f.subs[sub] = struct{}{}
	return sub
}

func (f *factoryFeed) leave(sub chan string) {
	f.mu.Lock()
	defer f.mu.Unlock()
	delete(f.subs, sub)
}
