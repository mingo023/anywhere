package devices

import (
	"slices"
	"time"
)

// EnsureLegacy stores the shared token of old phones as the legacy device,
// hashed like every other token, first in the list. A second call, or any
// call once the legacy grace has ended, is a no-op.
func (s *Store) EnsureLegacy(token string) error {
	if token == "" {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.graceEnded || s.index(LegacyID) >= 0 {
		return nil
	}
	s.devices = slices.Insert(s.devices, 0, Device{
		ID: LegacyID, Name: "Shared token (legacy)", Scopes: LegacyScopes,
		TokenHash: hash(token), CreatedAt: time.Now().UnixMilli(), Legacy: true,
	})
	return s.save()
}
