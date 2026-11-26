package e2e

import (
	"os"
	"path/filepath"
)

var tempRoot = os.TempDir()

const exeSuffix = ".exe"

// platformBins is empty: redteam is unix-only.
var platformBins [][2]string

// rebootSignal is a kill: Go can't send SIGTERM on Windows, and Task
// Scheduler's /End, which stops an installed pocketd, terminates it too.
// TerminateProcess exits 1.
var rebootSignal os.Signal = os.Kill

const rebootExit = 1

// harnessEnv points os.UserHomeDir, which reads USERPROFILE on Windows, at
// home. Restore runs in shellenv's LoginShell, as an installed pocketd's does.
func harnessEnv(home string) []string {
	return []string{"USERPROFILE=" + home}
}

// buildPlatformFakes hard-links anywhere.exe to pocketd.exe: a symlink needs
// a privilege, and pocketd tells its name from argv0 either way.
func buildPlatformFakes(dir string) {
	if err := os.Link(filepath.Join(dir, "pocketd.exe"), filepath.Join(dir, "anywhere.exe")); err != nil {
		panic(err)
	}
}
