package gitview

import (
	"cmp"
	"sync"
	"time"
)

const defaultEvery = 2 * time.Second

// watchers are the trees some conn watches, each polled by one goroutine
// that runs only while anyone watches it.
type watchers struct {
	mu    sync.Mutex
	next  int
	trees map[string]*watched
}

type watched struct {
	subs map[int]func(tree, rev string)
	rev  string
	stop chan struct{}
}

// Watch calls changed whenever one of trees' rev moves, until stop.
func (s *Service) Watch(trees []string, changed func(tree, rev string)) (stop func()) {
	w := &s.watch
	w.mu.Lock()
	defer w.mu.Unlock()
	w.next++
	id := w.next
	if w.trees == nil {
		w.trees = map[string]*watched{}
	}
	for _, t := range trees {
		tw := w.trees[t]
		if tw == nil {
			tw = &watched{subs: map[int]func(string, string){}, stop: make(chan struct{})}
			w.trees[t] = tw
			go s.poll(t, tw)
		}
		tw.subs[id] = changed
	}
	return sync.OnceFunc(func() {
		w.mu.Lock()
		defer w.mu.Unlock()
		for _, t := range trees {
			tw := w.trees[t]
			if tw == nil {
				continue
			}
			delete(tw.subs, id)
			if len(tw.subs) == 0 {
				close(tw.stop)
				delete(w.trees, t)
			}
		}
	})
}

func (s *Service) poll(tree string, tw *watched) {
	s.check(tree, tw)
	tick := time.NewTicker(cmp.Or(s.Every, defaultEvery))
	defer tick.Stop()
	for {
		select {
		case <-tw.stop:
			return
		case <-tick.C:
			s.check(tree, tw)
		}
	}
}

// check reads tree's rev and tells its watchers when it moved since the last read.
func (s *Service) check(tree string, tw *watched) {
	rev, err := s.Rev(tree)
	if err != nil {
		return
	}
	w := &s.watch
	w.mu.Lock()
	moved := tw.rev != "" && rev != tw.rev
	tw.rev = rev
	var subs []func(string, string)
	if moved {
		for _, f := range tw.subs {
			subs = append(subs, f)
		}
	}
	w.mu.Unlock()
	for _, f := range subs {
		f(tree, rev)
	}
}

// Kick re-reads a watched tree now, after a write.
func (s *Service) Kick(tree string) {
	s.watch.mu.Lock()
	tw := s.watch.trees[tree]
	s.watch.mu.Unlock()
	if tw != nil {
		s.check(tree, tw)
	}
}

// Watching is how many trees are polled now.
func (s *Service) Watching() int {
	s.watch.mu.Lock()
	defer s.watch.mu.Unlock()
	return len(s.watch.trees)
}
