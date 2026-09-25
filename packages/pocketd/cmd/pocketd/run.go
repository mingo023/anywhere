package main

import (
	"errors"
	"os"
	"os/signal"
	"syscall"

	"golang.org/x/term"

	"pocketd/internal/ops"
)

func run(sock, id, cmd string, args []string) (int, error) {
	c, err := ops.Dial(sock)
	if err != nil {
		return 1, err
	}
	defer c.Close()
	cols, rows, _ := term.GetSize(int(os.Stdout.Fd()))
	if id == "" {
		cwd, _ := os.Getwd()
		if err := c.Send(ops.Msg{Op: "spawn", Cmd: cmd, Args: args, Cwd: cwd, Env: os.Environ(), Cols: cols, Rows: rows}); err != nil {
			return 1, err
		}
		m, err := c.Recv()
		if err != nil {
			return 1, err
		}
		if m.Ev == "error" {
			return 1, errors.New(m.Error)
		}
		id = m.ID
	} else if err := c.Send(ops.Msg{Op: "resize", ID: id, Cols: cols, Rows: rows}); err != nil {
		return 1, err
	}
	if err := c.Send(ops.Msg{Op: "attach", ID: id, TTY: true}); err != nil {
		return 1, err
	}

	state, err := term.MakeRaw(int(os.Stdin.Fd()))
	if err != nil {
		return 1, err
	}
	defer term.Restore(int(os.Stdin.Fd()), state)

	winch := make(chan os.Signal, 1)
	signal.Notify(winch, syscall.SIGWINCH)
	go func() {
		for range winch {
			cols, rows, _ := term.GetSize(int(os.Stdout.Fd()))
			if c.Send(ops.Msg{Op: "resize", ID: id, Cols: cols, Rows: rows}) != nil {
				return
			}
		}
	}()
	go func() {
		buf := make([]byte, 4096)
		for {
			n, err := os.Stdin.Read(buf)
			if err != nil {
				return
			}
			if c.Send(ops.Msg{Op: "input", ID: id, Data: buf[:n]}) != nil {
				return
			}
		}
	}()

	for {
		m, err := c.Recv()
		if err != nil {
			return 1, err
		}
		switch m.Ev {
		case "snapshot":
			os.Stdout.Write([]byte("\x1b[H\x1b[2J"))
			os.Stdout.Write(m.Data)
		case "output":
			os.Stdout.Write(m.Data)
		case "exit":
			return m.Code, nil
		case "error":
			return 1, errors.New(m.Error)
		}
	}
}
