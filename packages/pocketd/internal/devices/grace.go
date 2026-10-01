package devices

import (
	"slices"
	"time"
)

// Grace is how long old phones keep the shared token after the first serve
// that retires it.
const Grace = 7 * 24 * time.Hour

func (d Device) graceOver(now time.Time) bool {
	return d.GraceEndsAt != 0 && now.UnixMilli() >= d.GraceEndsAt
}

// StartGrace starts the legacy device's grace, once, and returns when it
// ends: zero when there is no legacy device.
func (s *Store) StartGrace(now time.Time) (time.Time, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	n := s.index(LegacyID)
	if n < 0 {
		return time.Time{}, nil
	}
	if s.devices[n].GraceEndsAt == 0 {
		s.devices[n].GraceEndsAt = now.Add(Grace).UnixMilli()
		if err := s.save(); err != nil {
			s.devices[n].GraceEndsAt = 0
			return time.Time{}, err
		}
	}
	return time.UnixMilli(s.devices[n].GraceEndsAt), nil
}

// EndGrace reports the grace over even when forgetting the device fails to
// save: Lookup refuses the old token either way.
func (s *Store) EndGrace(now time.Time) (bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	n := s.index(LegacyID)
	if n < 0 || !s.devices[n].graceOver(now) {
		return false, nil
	}
	before := s.devices
	s.devices, s.graceEnded = slices.Delete(slices.Clone(before), n, n+1), true
	if err := s.save(); err != nil {
		s.devices, s.graceEnded = before, false
		return true, err
	}
	return true, nil
}
