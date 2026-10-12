package proc

import (
	"bytes"
	"errors"
	"os"
	"slices"
	"strconv"
	"strings"
)

// stat is the part of /proc/<pid>/stat this package uses.
type stat struct {
	comm                string
	state               byte
	ppid, pgrp, session int
}

// readStat parses /proc/<pid>/stat. comm sits in parentheses and may itself
// hold spaces and parentheses, so the fields after it start at the last ')'.
func readStat(pid int) (stat, error) {
	b, err := os.ReadFile("/proc/" + strconv.Itoa(pid) + "/stat")
	if err != nil {
		return stat{}, err
	}
	return parseStat(b)
}

func parseStat(b []byte) (stat, error) {
	open, end := bytes.IndexByte(b, '('), bytes.LastIndexByte(b, ')')
	if open < 0 || end < open {
		return stat{}, errors.New("proc: malformed stat")
	}
	f := strings.Fields(string(b[end+1:]))
	if len(f) < 4 || len(f[0]) != 1 {
		return stat{}, errors.New("proc: malformed stat")
	}
	var s stat
	s.comm, s.state = string(b[open+1:end]), f[0][0]
	var err error
	if s.ppid, err = strconv.Atoi(f[1]); err != nil {
		return stat{}, err
	}
	if s.pgrp, err = strconv.Atoi(f[2]); err != nil {
		return stat{}, err
	}
	if s.session, err = strconv.Atoi(f[3]); err != nil {
		return stat{}, err
	}
	return s, nil
}

// pids lists every process /proc shows.
func pids() ([]int, error) {
	ents, err := os.ReadDir("/proc")
	if err != nil {
		return nil, err
	}
	var ps []int
	for _, e := range ents {
		if pid, err := strconv.Atoi(e.Name()); err == nil {
			ps = append(ps, pid)
		}
	}
	return ps, nil
}

func Members(pgid int) ([]int, error) {
	all, err := pids()
	if err != nil {
		return nil, err
	}
	var out []int
	for _, pid := range all {
		if s, err := readStat(pid); err == nil && s.pgrp == pgid && s.state != 'Z' {
			out = append(out, pid)
		}
	}
	slices.Sort(out)
	return out, nil
}

// Read falls back to the kernel's name when argv is unreadable: a zombie, or
// a kernel thread. environ is readable only for the same user.
func Read(pid int) (Proc, error) {
	dir := "/proc/" + strconv.Itoa(pid) + "/"
	if b, err := os.ReadFile(dir + "cmdline"); err == nil && len(b) > 0 {
		if argv := split(b); len(argv) > 0 && argv[0] != "" {
			env, _ := os.ReadFile(dir + "environ")
			return Proc{Pid: pid, Argv: argv, Env: split(env)}, nil
		}
	}
	s, err := readStat(pid)
	if err != nil {
		return Proc{}, err
	}
	return Proc{Pid: pid, Argv: []string{s.comm}}, nil
}

func Parent(pid int) (int, error) {
	s, err := readStat(pid)
	if err != nil {
		return 0, err
	}
	return s.ppid, nil
}

// split reads NUL-terminated strings. A process that rewrote its title in
// place leaves NUL fill, so trailing empty strings are dropped.
func split(b []byte) []string {
	b = bytes.TrimRight(b, "\x00")
	if len(b) == 0 {
		return nil
	}
	var out []string
	for s := range bytes.SplitSeq(b, []byte{0}) {
		out = append(out, string(s))
	}
	return out
}
