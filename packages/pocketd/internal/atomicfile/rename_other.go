//go:build !windows

package atomicfile

import "os"

func rename(from, to string) error { return os.Rename(from, to) }
