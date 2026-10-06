package main

import (
	"pocketd/internal/handoff"
	"pocketd/internal/vt"
)

// handoffCheck is the dry run an upgrading pocketd runs with the new binary:
// the file reads, every fd in it came through, and every screen replays.
func handoffCheck(path string) error {
	f, err := handoff.Read(path)
	if err != nil {
		return err
	}
	if err := handoff.Check(f); err != nil {
		return err
	}
	for _, t := range f.Terminals {
		v, err := vt.New(t.Saved.Cols, t.Saved.Rows, func([]byte) {})
		if err != nil {
			return err
		}
		v.Write(t.Screen)
		v.Free()
	}
	return nil
}
