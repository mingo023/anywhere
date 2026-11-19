//go:build unix

package main

import (
	"encoding/json"
	"os"
	"os/signal"
	"strconv"
	"syscall"

	"golang.org/x/sys/unix"

	"pocketd/internal/holder"
)

// hold is `pocketd hold`: it runs one Terminal's process for pocketd (ADR 0008).
func hold() error {
	signal.Ignore(syscall.SIGHUP)
	var spec holder.Spec
	if err := json.NewDecoder(os.Stdin).Decode(&spec); err != nil {
		return err
	}
	var adopted *os.File
	keep := 2
	if spec.AdoptPid > 0 {
		adopted, keep = os.NewFile(3, "/dev/ptmx"), 3
	}
	closeInherited(keep)
	return holder.Serve(spec, adopted, os.Stdout)
}

// closeInherited closes files and sockets above keep: an upgrade's lock or
// MCP fd leaked in would pin pocketd.lock or the MCP port for this holder's life.
func closeInherited(keep int) {
	entries, _ := os.ReadDir("/dev/fd")
	for _, e := range entries {
		fd, err := strconv.Atoi(e.Name())
		if err != nil || fd <= keep {
			continue
		}
		var st unix.Stat_t
		if unix.Fstat(fd, &st) != nil {
			continue
		}
		if t := st.Mode & unix.S_IFMT; t == unix.S_IFREG || t == unix.S_IFSOCK {
			unix.Close(fd)
		}
	}
}
