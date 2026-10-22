package accounts

import "slices"

// Peak is the larger of an account's 5-hour and weekly use, in percent; 0
// when its usage isn't known.
func (u *Usage) Peak() float64 {
	if u == nil {
		return 0
	}
	peak := 0.0
	for _, w := range []*Window{u.Session, u.Week} {
		if w != nil {
			peak = max(peak, w.Percent)
		}
	}
	return peak
}

// Limited is whether the account has used up one of its limits.
func (u *Usage) Limited() bool { return u.Peak() >= 100 }

// Pick is the account a new session of provider runs on: id when the launch
// names one, else prefer, the Project's choice, else the default, else the
// built-in one. One signed out gives way to the next. One at its limit gives
// way to the one with the most left, when switching at a limit is on.
func (s *Store) Pick(provider, id, prefer string) (string, error) {
	if id != "" {
		if _, err := s.Resolve(provider, id); err != nil {
			return "", err
		}
	}
	all := s.List(func(string, string) int { return 0 })
	listed := func(id string) *Account {
		i := slices.IndexFunc(all, func(a Account) bool { return a.Provider == provider && a.ID == id && !a.Pending })
		if i < 0 {
			return nil
		}
		return &all[i]
	}
	def, _ := s.Resolve(provider, "")
	picked := BuiltIn
	for _, c := range []string{id, prefer, def} {
		if a := listed(c); a != nil && a.SignedIn {
			picked = c
			break
		}
	}
	if a := listed(picked); a != nil && a.Usage.Limited() && s.AutoSwitch() {
		if best, ok := s.Best(provider, picked); ok {
			return best, nil
		}
	}
	return picked, nil
}

// Best is the signed-in account of provider other than except with the most
// left of its limits, as of the last usage read; false when every one is at
// a limit.
func (s *Store) Best(provider, except string) (string, bool) {
	best, ok, peak := "", false, 0.0
	for _, a := range s.List(func(string, string) int { return 0 }) {
		if a.Provider != provider || a.ID == except || a.Pending || !a.SignedIn || a.Usage.Limited() {
			continue
		}
		if p := a.Usage.Peak(); !ok || p < peak {
			best, ok, peak = a.ID, true, p
		}
	}
	return best, ok
}

// AutoSwitch is whether an agent at its account's limit moves to another;
// on until turned off.
func (s *Store) AutoSwitch() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	x := s.load()
	return x.AutoSwitch == nil || *x.AutoSwitch
}

func (s *Store) SetAutoSwitch(on bool) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	x := s.load()
	x.AutoSwitch = &on
	return s.save(x)
}
