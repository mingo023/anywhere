// Package logfile is an append-only file that rotates by size.
package logfile

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
)

type File struct {
	mu   sync.Mutex
	path string
	max  int64
	keep int
	f    *os.File
	size int64
}

// Open appends to path. A write that would pass max first shifts path to
// path.1, path.1 to path.2, and so on, dropping the one past keep.
func Open(path string, max int64, keep int) (*File, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_APPEND, 0o600)
	if err != nil {
		return nil, err
	}
	st, err := f.Stat()
	if err != nil {
		f.Close()
		return nil, err
	}
	return &File{path: path, max: max, keep: keep, f: f, size: st.Size()}, nil
}

func (l *File) Write(p []byte) (int, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.size > 0 && l.size+int64(len(p)) > l.max {
		if err := l.rotate(); err != nil {
			return 0, err
		}
	}
	n, err := l.f.Write(p)
	l.size += int64(n)
	return n, err
}

// rotate closes the file before renaming it, as Windows refuses to rename an open file. Where
// another process holds it open the rename still fails: it then keeps appending, and tries
// again only after another max bytes, so the older logs aren't shifted on every write.
func (l *File) rotate() error {
	l.f.Close()
	flag := os.O_WRONLY | os.O_CREATE | os.O_APPEND
	if os.Rename(l.path, l.path+".rotating") == nil {
		for n := l.keep; n > 1; n-- {
			os.Rename(fmt.Sprintf("%s.%d", l.path, n-1), fmt.Sprintf("%s.%d", l.path, n))
		}
		os.Rename(l.path+".rotating", l.path+".1")
		flag |= os.O_TRUNC
	}
	f, err := os.OpenFile(l.path, flag, 0o600)
	if err != nil {
		return err
	}
	l.f, l.size = f, 0
	return nil
}

func (l *File) Close() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.f.Close()
}
