package github

import (
	"context"
	"errors"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/worktree"
)

const (
	actTimeout  = time.Minute
	pushTimeout = 2 * time.Minute
	// treesEvery is how long the list of trees to poll is kept, since listing
	// runs git once per Project.
	treesEvery = 30 * time.Second
)

// pushArgv is the desktop's PUSH (crates/git/src/git.rs): a branch never
// pushed gets its upstream on the way.
var pushArgv = []string{"git", "-c", "push.autoSetupRemote=true", "push"}

// Error is a failed verb with protocol.md's code: gh_failed, gh_missing,
// gh_signed_out, or git_failed for the push before a create.
type Error struct{ Code, Message, Detail string }

func (e *Error) Error() string { return e.Message }

// Service serves the gh.v1 verbs from one cache that a poller keeps fresh
// while any phone listens: one gh at a time, the trees phones watch every
// WatchedEvery and the rest every OthersEvery.
type Service struct {
	env Env
	reg *registry.Registry
	now func() time.Time

	mu       sync.Mutex
	cache    *cache
	trees    []string
	treesAt  time.Time
	watchers map[string]*watcher
	cancel   context.CancelFunc
	wake     chan struct{}
}

type watcher struct {
	trees   []string
	changed func(tree string)
}

func New(env Env, reg *registry.Registry) *Service {
	return &Service{env: env, reg: reg, now: time.Now, cache: newCache(), watchers: map[string]*watcher{}, wake: make(chan struct{}, 1)}
}

// Listen adds a phone, told through changed when the PR of a tree it watches
// changes. The poller runs while any phone listens.
func (s *Service) Listen(key string, changed func(tree string)) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if w := s.watchers[key]; w != nil {
		w.changed = changed
		return
	}
	s.watchers[key] = &watcher{changed: changed}
	if s.cancel == nil {
		ctx, cancel := context.WithCancel(context.Background())
		s.cancel = cancel
		go s.poll(ctx)
	}
}

// Watch replaces the trees key watches; [] stops.
func (s *Service) Watch(key string, trees []string) {
	s.mu.Lock()
	if w := s.watchers[key]; w != nil {
		w.trees = slices.Clone(trees)
	}
	s.mu.Unlock()
	s.poke()
}

// Leave drops a phone; the last one stops the poller.
func (s *Service) Leave(key string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.watchers, key)
	if len(s.watchers) == 0 && s.cancel != nil {
		s.cancel()
		s.cancel = nil
	}
}

func (s *Service) poke() {
	select {
	case s.wake <- struct{}{}:
	default:
	}
}

func (s *Service) poll(ctx context.Context) {
	tick := time.NewTicker(time.Second)
	defer tick.Stop()
	for ctx.Err() == nil {
		if tree, started, ok := s.next(); ok {
			pr, problem := s.ask(ctx, tree)
			// A run killed because the last phone left says nothing about the PR.
			if ctx.Err() != nil {
				s.mu.Lock()
				s.cache.dropRun(tree)
				s.mu.Unlock()
				return
			}
			s.mu.Lock()
			changed := s.cache.apply(tree, started, pr, problem, s.now())
			s.mu.Unlock()
			if changed {
				s.changed(tree, problem != "")
			}
			continue
		}
		select {
		case <-ctx.Done():
			return
		case <-s.wake:
		case <-tick.C:
		}
	}
}

// next starts the run of the next due tree.
func (s *Service) next() (string, time.Time, bool) {
	now := s.now()
	s.mu.Lock()
	stale := now.Sub(s.treesAt) >= treesEvery
	s.mu.Unlock()
	if stale {
		trees := pollable(s.reg.Load())
		s.mu.Lock()
		s.trees, s.treesAt = trees, now
		s.mu.Unlock()
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	watched := map[string]bool{}
	for _, w := range s.watchers {
		for _, t := range w.trees {
			watched[t] = true
		}
	}
	tree, ok := s.cache.due(watched, s.trees, now)
	if ok {
		s.cache.start(tree, now)
	}
	return tree, now, ok
}

// pollable is every Worktree of every Project but the main ones and the
// detached ones, sorted, as the desktop polls.
func pollable(f registry.File) []string {
	var trees []string
	for _, p := range f.Projects {
		ws, _ := worktree.List(p)
		for _, w := range ws {
			if !w.Main && w.Branch != "" {
				trees = append(trees, w.Path)
			}
		}
	}
	slices.Sort(trees)
	return trees
}

// changed tells the phones watching tree; a problem with gh is every tree's.
func (s *Service) changed(tree string, everyTree bool) {
	s.mu.Lock()
	var tell []func()
	for _, w := range s.watchers {
		for _, t := range w.trees {
			if everyTree || t == tree {
				tell = append(tell, func() { w.changed(t) })
			}
		}
	}
	s.mu.Unlock()
	for _, f := range tell {
		f()
	}
}

// ask runs view in tree, then the review threads of an open PR. Threads that
// fail to load keep the ones already shown rather than emptying the list.
func (s *Service) ask(ctx context.Context, tree string) (*PR, Problem) {
	out, err := s.env.run(ctx, tree, view, "", ghTurn)
	pr, problem := viewResult(out, err)
	if pr == nil || pr.State != "OPEN" {
		return pr, problem
	}
	out, err = s.env.run(ctx, tree, threadsArgv(pr.Number), "", ghTurn)
	threads, ok := threadsResult(out)
	if err != nil || !ok {
		s.mu.Lock()
		if old, _ := s.cache.get(tree); old != nil {
			threads = old.Threads
		}
		s.mu.Unlock()
	}
	pr.Threads = threads
	return pr, ""
}

// Status checks gh as tree's Project account, or as the active one without a tree.
func (s *Service) Status(ctx context.Context, tree string) Status {
	version, verr := s.env.run(ctx, "/", versionArgv, "", actTimeout)
	dir := tree
	if dir == "" {
		dir = "/"
	}
	// gh auth status writes to stderr.
	auth, authErr, aerr := s.env.both(ctx, dir, authArgv, "", actTimeout)
	st := status(version, verr, auth+authErr, aerr)
	s.mu.Lock()
	s.cache.checked(st)
	s.mu.Unlock()
	if st.State == "ready" {
		s.poke()
	}
	return st
}

// PR is tree's PR: the cached one while it is fresh for a watched tree, else
// gh is asked now. A missing or signed-out gh is its problem.
func (s *Service) PR(ctx context.Context, tree string) (*proto.PR, Problem) {
	now := s.now()
	s.mu.Lock()
	a, had := s.cache.byTree[tree]
	problem := s.cache.problem
	if problem != "" && now.Sub(s.cache.problemAt) >= OthersEvery {
		problem = ""
	}
	s.mu.Unlock()
	switch {
	case had && now.Sub(a.at) < WatchedEvery:
		return wire(a.pr), ""
	case problem != "" && !had:
		return nil, problem
	}
	pr, problem := s.ask(ctx, tree)
	if ctx.Err() != nil {
		return nil, ""
	}
	s.mu.Lock()
	changed := s.cache.record(tree, pr, problem, s.now())
	if problem != "" {
		if old, ok := s.cache.get(tree); ok {
			pr, problem = old, ""
		}
	}
	s.mu.Unlock()
	if changed {
		s.changed(tree, problem != "")
	}
	return wire(pr), problem
}

func wire(pr *PR) *proto.PR {
	if pr == nil {
		return nil
	}
	return pr.Wire()
}

// PRs are the cached PRs of project's Worktrees, in the order they are listed.
func (s *Service) PRs(project string) []proto.PRItem {
	ws, _ := worktree.List(project)
	s.mu.Lock()
	defer s.mu.Unlock()
	items := []proto.PRItem{}
	for _, w := range ws {
		if pr, _ := s.cache.get(w.Path); pr != nil {
			items = append(items, pr.Item(w.Path))
		}
	}
	return items
}

// act runs a gh verb in tree; its failure is coded for the phone.
func (s *Service) act(ctx context.Context, tree string, argv []string, stdin string) (string, error) {
	out, err := s.env.run(ctx, tree, argv, stdin, actTimeout)
	if err == nil {
		return out, nil
	}
	switch problemOf(err) {
	case Missing:
		return "", &Error{Code: proto.CodeGhMissing, Message: "Install GitHub CLI (gh) to work with pull requests"}
	case SignedOut:
		return "", &Error{Code: proto.CodeGhSignedOut, Message: "Run gh auth login to work with pull requests"}
	}
	msg, detail := failure(err)
	return "", &Error{Code: proto.CodeGhFailed, Message: msg, Detail: detail}
}

// failure is err's first line and, for a run that said more, all of it.
func failure(err error) (string, string) {
	text := strings.TrimSpace(err.Error())
	line, _, more := strings.Cut(text, "\n")
	if line == "" {
		line = "gh failed"
	}
	if !more {
		text = ""
	}
	return line, text
}

// after records what an act did to tree's PR and has the poller ask gh again soon.
func (s *Service) after(tree string, edit func(*PR)) {
	s.mu.Lock()
	if edit != nil {
		s.cache.edit(tree, edit)
	}
	s.cache.expire(tree, s.now())
	s.mu.Unlock()
	s.changed(tree, false)
	s.poke()
}

// Create pushes tree's branch, then opens a PR for it into base, or the
// repo's default branch without one.
func (s *Service) Create(ctx context.Context, tree, title, body, base string, draft bool) (int, string, error) {
	if _, err := s.env.run(ctx, tree, pushArgv, "", pushTimeout); err != nil {
		msg, detail := failure(err)
		if f, ok := errors.AsType[*Failure](err); ok {
			detail = strings.TrimSpace(f.Stderr)
		}
		return 0, "", &Error{Code: proto.CodeGitFailed, Message: msg, Detail: detail}
	}
	out, err := s.act(ctx, tree, createArgv(base, title, draft), body)
	if err != nil {
		return 0, "", err
	}
	number, url, ok := created(out)
	if !ok {
		return 0, "", &Error{Code: proto.CodeGhFailed, Message: "gh opened the PR but printed no link to it"}
	}
	s.mu.Lock()
	s.cache.put(tree, &PR{Number: number, Title: title, URL: url, State: "OPEN", Draft: draft, Base: baseBranch(base), Mergeable: "UNKNOWN"}, s.now())
	s.mu.Unlock()
	s.changed(tree, false)
	s.poke()
	return number, url, nil
}

// Merge merges PR number by method, never deleting its branch.
func (s *Service) Merge(ctx context.Context, tree string, number int, method string) error {
	if _, err := s.act(ctx, tree, mergeArgv(number, method), ""); err != nil {
		return err
	}
	s.after(tree, func(pr *PR) { pr.State = "MERGED" })
	return nil
}

func (s *Service) Ready(ctx context.Context, tree string, number int) error {
	if _, err := s.act(ctx, tree, readyArgv(number), ""); err != nil {
		return err
	}
	s.after(tree, func(pr *PR) { pr.Draft = false })
	return nil
}

func (s *Service) Reply(ctx context.Context, tree, thread, body string) error {
	if _, err := s.act(ctx, tree, replyArgv(thread, body), ""); err != nil {
		return err
	}
	s.after(tree, nil)
	return nil
}

func (s *Service) Resolve(ctx context.Context, tree, thread string, resolved bool) error {
	if _, err := s.act(ctx, tree, resolveArgv(thread, resolved), ""); err != nil {
		return err
	}
	s.after(tree, func(pr *PR) {
		pr.Threads = slices.Clone(pr.Threads)
		for i := range pr.Threads {
			if pr.Threads[i].ID == thread {
				pr.Threads[i].Resolved = resolved
			}
		}
	})
	return nil
}
