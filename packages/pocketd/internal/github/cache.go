package github

import (
	"reflect"
	"time"
)

const (
	// WatchedEvery is how often a tree a phone watches is asked about.
	WatchedEvery = 30 * time.Second
	// OthersEvery is how often every other tree is, and a missing or signed-out gh asked again.
	OthersEvery = 5 * time.Minute
	// ghTurn is how long one gh call may hang before it gives up its turn.
	ghTurn = time.Minute
)

type answer struct {
	at time.Time
	pr *PR
}

// cache is each tree's PR as gh last reported it and when it was asked, the
// desktop's PullRequests (crates/pocket/src/git_ui/pull_requests.rs).
type cache struct {
	byTree map[string]answer
	// running is the tree gh is asked about, since runningAt.
	running   string
	runningAt time.Time
	problem   Problem
	problemAt time.Time
}

func newCache() *cache { return &cache{byTree: map[string]answer{}} }

// due is the tree to ask gh about next: a watched one every WatchedEvery, the
// others every OthersEvery, in trees' order, one at a time.
func (c *cache) due(watched map[string]bool, trees []string, now time.Time) (string, bool) {
	if c.running != "" && now.Sub(c.runningAt) < ghTurn || c.problem != "" && now.Sub(c.problemAt) < OthersEvery {
		return "", false
	}
	stale := func(t string, every time.Duration) bool {
		a, ok := c.byTree[t]
		return !ok || now.Sub(a.at) >= every
	}
	for _, t := range trees {
		if watched[t] && stale(t, WatchedEvery) {
			return t, true
		}
	}
	for _, t := range trees {
		if stale(t, OthersEvery) {
			return t, true
		}
	}
	return "", false
}

// checked records a gh status check; a ready one lets polling resume at once.
func (c *cache) checked(s Status) {
	if s.State == "ready" {
		c.problem = ""
	}
}

// start marks gh as asked about tree since started, the run apply expects an answer from.
func (c *cache) start(tree string, started time.Time) {
	c.running, c.runningAt = tree, started
}

// apply records what the gh run for tree begun at started said, unless that
// run was forgotten or superseded. True when it changes what's shown.
func (c *cache) apply(tree string, started time.Time, pr *PR, problem Problem, now time.Time) bool {
	if c.running != tree || !c.runningAt.Equal(started) {
		return false
	}
	c.running = ""
	return c.record(tree, pr, problem, now)
}

// record takes an answer for tree asked for outside the poll, as a phone's
// gh.pr is; an answer of the poll still running for tree no longer lands.
func (c *cache) record(tree string, pr *PR, problem Problem, now time.Time) bool {
	c.dropRun(tree)
	if problem != "" {
		changed := c.problem != problem
		c.problem, c.problemAt = problem, now
		return changed
	}
	old, had := c.byTree[tree]
	changed := c.problem != "" || !had || !reflect.DeepEqual(old.pr, pr)
	c.problem = ""
	c.byTree[tree] = answer{now, pr}
	return changed
}

func (c *cache) get(tree string) (*PR, bool) {
	a, ok := c.byTree[tree]
	return a.pr, ok
}

// forget drops tree's PR, and the answer of any run in flight for it, so the next poll asks gh again.
func (c *cache) forget(tree string) {
	delete(c.byTree, tree)
	c.dropRun(tree)
}

// put shows pr for tree at once, as when it was just opened, until the next poll asks gh.
func (c *cache) put(tree string, pr *PR, now time.Time) {
	c.byTree[tree] = answer{now, pr}
	c.expire(tree, now)
}

// edit changes tree's PR after acting on it, so an answer asked for before can't undo it.
func (c *cache) edit(tree string, f func(*PR)) {
	if a, ok := c.byTree[tree]; ok && a.pr != nil {
		pr := *a.pr
		f(&pr)
		c.byTree[tree] = answer{a.at, &pr}
	}
	c.dropRun(tree)
}

// expire keeps tree's PR shown but asks gh about it at the next poll.
func (c *cache) expire(tree string, now time.Time) {
	if a, ok := c.byTree[tree]; ok {
		a.at = now.Add(-OthersEvery)
		c.byTree[tree] = a
	}
	c.dropRun(tree)
}

func (c *cache) dropRun(tree string) {
	if c.running == tree {
		c.running = ""
	}
}
