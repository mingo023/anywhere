// Package awake keeps the Mac from idle sleep while an agent is working or needs you.
package awake

import (
	"context"
	"log"
	"time"
)

type Sleeper interface {
	Hold(reason string) error
	Release() error
}

type Keeper struct {
	S       Sleeper
	Enabled func() bool
	Linger  func() time.Duration
	After   func(time.Duration) <-chan time.Time
	OnHeld  func(bool)
}

// Run holds S while Enabled() and busy(), and releases it Linger() after busy() goes false.
// Enabled and Linger are read on each kick, so a changed setting applies at the next one.
// busy is checked on each kick, so Run costs nothing between status changes.
func (k *Keeper) Run(ctx context.Context, kick <-chan struct{}, busy func() bool) {
	held := false
	var linger <-chan time.Time
	set := func(on bool) {
		if on == held {
			return
		}
		var err error
		if on {
			err = k.S.Hold("Pocket: an agent is working")
		} else {
			err = k.S.Release()
		}
		if err != nil {
			log.Printf("keep awake: %v", err)
			return
		}
		held = on
		if k.OnHeld != nil {
			k.OnHeld(on)
		}
	}
	for {
		select {
		case <-ctx.Done():
			set(false)
			return
		case <-kick:
			if !k.Enabled() {
				linger = nil
				set(false)
			} else if busy() {
				linger = nil
				set(true)
			} else if held && linger == nil {
				linger = k.After(k.Linger())
			}
		case <-linger:
			linger = nil
			set(false)
		}
	}
}
