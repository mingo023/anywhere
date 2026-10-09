package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"strconv"
	"sync"
	"sync/atomic"
	"time"

	"pocketd/internal/config"
	"pocketd/internal/launch"
	"pocketd/internal/ops"
	"pocketd/internal/reach"
)

// hookExit reports a create wrapper's setup or agent exit. Like hook, it
// never fails: the wrapper goes on to the shell either way.
func hookExit(sock, phase, status string) error {
	pty := os.Getenv("POCKETD_PTY")
	code, err := strconv.Atoi(status)
	if pty == "" || err != nil {
		return nil
	}
	c, err := ops.Dial(sock)
	if err != nil {
		return nil
	}
	defer c.Close()
	if c.Send(ops.Msg{Op: "launch-exit", ID: pty, Text: phase, Code: code}) != nil {
		return nil
	}
	answered := make(chan struct{})
	go func() {
		c.Recv()
		close(answered)
	}()
	select {
	case <-answered:
	case <-time.After(2 * time.Second):
	}
	return nil
}

func configSet(sock, key, value string, out io.Writer) error {
	c, err := ops.Dial(sock)
	if err != nil {
		return fmt.Errorf("pocketd isn't running: %w", err)
	}
	defer c.Close()
	if err := c.Send(ops.Msg{Op: "config-set", Key: key, Text: value}); err != nil {
		return err
	}
	m, err := c.Recv()
	if err != nil {
		return err
	}
	if m.Ev != "ok" {
		return errors.New(m.Error)
	}
	fmt.Fprintf(out, "%s = %s\n", key, value)
	return nil
}

// networkSetter applies listen and port to the phone listener before saving
// them, so a port that won't bind is refused and the old listener goes on
// serving. Other keys go to rest.
func networkSetter(phones *atomic.Pointer[reach.Listener], settings *config.Settings, moved func(), rest func(key, value string) error) func(key, value string) error {
	var mu sync.Mutex
	return func(key, value string) error {
		switch key {
		case "listen":
			if err := config.ParseListen(value); err != nil {
				return err
			}
			mu.Lock()
			defer mu.Unlock()
			if err := settings.SetListen(value); err != nil {
				return err
			}
			phones.Load().SetMode(value)
		case "port":
			n, err := config.ParsePort(value)
			if err != nil {
				return err
			}
			mu.Lock()
			defer mu.Unlock()
			old := phones.Load().Port()
			if n != old {
				next, err := phones.Load().Move(n)
				if err != nil {
					return fmt.Errorf("port %d can't be used: %w", n, err)
				}
				phones.Store(next)
			}
			if err := settings.SetPort(n); err != nil {
				// Unsaved, the new port would last only until pocketd restarts, so go back to the one config.json names.
				if back, moveErr := phones.Load().Move(old); moveErr == nil {
					phones.Store(back)
				}
				return err
			}
		default:
			return rest(key, value)
		}
		moved()
		return nil
	}
}

// configSetter calls changed after each write, so the keep-awake keeper rereads its settings.
func configSetter(l *launch.Launcher, settings *config.Settings, changed func()) func(key, value string) error {
	return func(key, value string) error {
		if key == "phone.maxAccess" {
			return l.SetPhoneMaxAccess(value)
		}
		if key == "claude.command" || key == "codex.command" {
			if err := l.CheckCommand(value); err != nil {
				return err
			}
		}
		if err := settings.Set(key, value); err != nil {
			return err
		}
		changed()
		return nil
	}
}
