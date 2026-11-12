package proc

import (
	"strconv"

	"pocketd/internal/envdir"
)

// Marker is the env var that names the pocketd a Terminal process came from.
const Marker = "POCKETD_PARENT"

// Reap picks the pids a dead pocketd left behind. A session leader is
// spared: codex's shared app-server and tmux call setsid and serve work
// outside Pocket.
func Reap(self int, alive func(int) bool, ps []Proc) []int {
	var pids []int
	for _, p := range ps {
		parent := markedBy(p.Env)
		if parent <= 0 || parent == self || p.Sid == p.Pid || alive(parent) {
			continue
		}
		pids = append(pids, p.Pid)
	}
	return pids
}

func markedBy(env []string) int {
	pid, _ := strconv.Atoi(envdir.Value(env, Marker))
	return pid
}
