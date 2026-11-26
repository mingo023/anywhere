package atomicfile

import (
	"errors"
	"os"
	"time"

	"golang.org/x/sys/windows"
)

// rename retries for a moment while the target is held open: Windows refuses to replace a file another
// handle has open without FILE_SHARE_DELETE, which os.Open doesn't ask for.
func rename(from, to string) error {
	deadline := time.Now().Add(time.Second)
	for wait := time.Millisecond; ; wait = min(2*wait, 50*time.Millisecond) {
		err := os.Rename(from, to)
		if !errors.Is(err, windows.ERROR_ACCESS_DENIED) && !errors.Is(err, windows.ERROR_SHARING_VIOLATION) || time.Now().After(deadline) {
			return err
		}
		time.Sleep(wait)
	}
}
