package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"strconv"
	"time"

	"pocketd/internal/config"
	"pocketd/internal/launch"
	"pocketd/internal/ops"
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

func configSetter(l *launch.Launcher, settings *config.Settings) func(key, value string) error {
	return func(key, value string) error {
		switch key {
		case "phone.maxAccess":
			return l.SetPhoneMaxAccess(value)
		case "restore.resumeAgents":
			on, err := strconv.ParseBool(value)
			if err != nil {
				return err
			}
			return settings.SetResumeAgents(on)
		}
		return fmt.Errorf("unknown key %q", key)
	}
}
