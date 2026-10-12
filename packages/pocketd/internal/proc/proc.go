// Package proc reads the processes behind a PTY from the kernel.
package proc

type Proc struct {
	Pid  int
	Sid  int // set by Orphans only
	Argv []string
	Env  []string
}
