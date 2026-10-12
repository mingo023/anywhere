package proc

import (
	"bufio"
	"os"
	"slices"
	"strconv"
	"strings"
)

// Session lists the process groups of sid's live members.
func Session(sid int) ([]int, error) {
	all, err := pids()
	if err != nil {
		return nil, err
	}
	uid := os.Getuid()
	var pgids []int
	for _, pid := range all {
		s, err := readStat(pid)
		if err != nil || s.state == 'Z' || s.session != sid || ruid(pid) != uid {
			continue
		}
		if !slices.Contains(pgids, s.pgrp) {
			pgids = append(pgids, s.pgrp)
		}
	}
	return pgids, nil
}

// Orphans lists uid's live processes that init or a systemd --user subreaper
// adopted, with their env where readable.
func Orphans(uid int) ([]Proc, error) {
	all, err := pids()
	if err != nil {
		return nil, err
	}
	var ps []Proc
	for _, pid := range all {
		s, err := readStat(pid)
		if err != nil || s.state == 'Z' || !adopter(s.ppid) || ruid(pid) != uid {
			continue
		}
		p, err := Read(pid)
		if err != nil {
			continue
		}
		p.Sid = s.session
		ps = append(ps, p)
	}
	return ps, nil
}

func adopter(pid int) bool {
	if pid == 1 {
		return true
	}
	s, err := readStat(pid)
	return err == nil && s.comm == "systemd"
}

// ruid is the real uid from /proc/<pid>/status, or -1 if unreadable.
func ruid(pid int) int {
	f, err := os.Open("/proc/" + strconv.Itoa(pid) + "/status")
	if err != nil {
		return -1
	}
	defer f.Close()
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		if v, ok := strings.CutPrefix(sc.Text(), "Uid:"); ok {
			if f := strings.Fields(v); len(f) > 0 {
				if u, err := strconv.Atoi(f[0]); err == nil {
					return u
				}
			}
			return -1
		}
	}
	return -1
}
