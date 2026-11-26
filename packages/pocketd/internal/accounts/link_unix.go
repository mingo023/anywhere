//go:build unix

package accounts

import "os"

func link(src, dst string) error {
	return os.Symlink(src, dst)
}
