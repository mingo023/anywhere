//go:build unix

package main

import (
	"os"
	"os/signal"
	"syscall"
)

func notifyResize(c chan<- os.Signal) { signal.Notify(c, syscall.SIGWINCH) }

func detachConsole() {}
