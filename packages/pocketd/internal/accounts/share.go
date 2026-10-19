package accounts

import (
	"errors"
	"os"
	"path/filepath"
)

// Share links account id's folder to the built-in one for what its login
// shares, so its agents keep the user's settings, skills, hooks and MCP
// servers, then runs the login's shareMore. It leaves anything the account
// already has, and never touches credentials. A provider with no login shares nothing.
func (s *Store) Share(provider, id string) error {
	if id == BuiltIn {
		return nil
	}
	s.share.Lock()
	defer s.share.Unlock()
	src, dst := s.Dir(provider, BuiltIn), s.Dir(provider, id)
	if err := os.MkdirAll(dst, 0o700); err != nil {
		return err
	}
	l, ok := logins[provider]
	if !ok {
		return nil
	}
	var errs []error
	for _, name := range l.shared() {
		if _, err := os.Stat(filepath.Join(src, name)); err != nil {
			continue
		}
		if name == l.history() {
			if err := absorb(filepath.Join(dst, name), filepath.Join(src, name)); err != nil {
				errs = append(errs, err)
				continue
			}
		}
		if _, err := os.Lstat(filepath.Join(dst, name)); err == nil {
			continue
		}
		errs = append(errs, os.Symlink(filepath.Join(src, name), filepath.Join(dst, name)))
	}
	errs = append(errs, l.shareMore(s, id))
	return errors.Join(errs...)
}

// absorb moves what the real folder from holds into to, leaving anything to
// already has, and removes from once it is empty. A link or a missing from
// is left as it is.
func absorb(from, to string) error {
	st, err := os.Lstat(from)
	if err != nil || !st.IsDir() {
		return nil
	}
	if err := mergeInto(from, to); err != nil {
		return err
	}
	return os.Remove(from)
}

func mergeInto(from, to string) error {
	entries, err := os.ReadDir(from)
	if err != nil {
		return err
	}
	for _, e := range entries {
		f, t := filepath.Join(from, e.Name()), filepath.Join(to, e.Name())
		if _, err := os.Lstat(t); err != nil {
			if err := os.Rename(f, t); err != nil {
				return err
			}
			continue
		}
		if e.IsDir() {
			if err := mergeInto(f, t); err != nil {
				return err
			}
			os.Remove(f)
		}
	}
	return nil
}
