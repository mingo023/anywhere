package hostop

import (
	"bytes"
	"context"
	"errors"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// ReadMax is what Read returns of a file when the caller names no max.
const ReadMax = 16 << 20

type Entry struct {
	Name string `json:"name"`
	// Dir is true for a link to a directory too.
	Dir     bool  `json:"dir"`
	Link    bool  `json:"link"`
	Size    int64 `json:"size"`
	Mtime   int64 `json:"mtime"` // unix ms
	Ignored bool  `json:"ignored"`
}

type Info struct {
	Exists bool
	Dir    bool
	Size   int64
	Mtime  int64 // unix ms
}

// List is dir's entries, unsorted. Ignored marks what git ignores, so it runs
// git from env's PATH when dir is in a repository.
func List(ctx context.Context, env []string, dir string) ([]Entry, error) {
	des, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	entries := make([]Entry, 0, len(des))
	for _, de := range des {
		e := Entry{Name: de.Name(), Dir: de.IsDir(), Link: de.Type()&fs.ModeSymlink != 0}
		info, err := de.Info()
		if e.Link {
			info, err = os.Stat(filepath.Join(dir, e.Name))
			e.Dir = err == nil && info.IsDir()
		}
		if err == nil {
			e.Size, e.Mtime = info.Size(), info.ModTime().UnixMilli()
		}
		entries = append(entries, e)
	}
	ignored := ignoredBy(ctx, env, dir, entries)
	for i := range entries {
		entries[i].Ignored = ignored[entries[i].Name]
	}
	return entries, nil
}

// ignoredBy asks git which entries it ignores; none outside a repository.
// A directory goes with a trailing slash so a "build/" pattern matches it.
func ignoredBy(ctx context.Context, env []string, dir string, entries []Entry) map[string]bool {
	var in bytes.Buffer
	for _, e := range entries {
		in.WriteString(e.Name)
		if e.Dir {
			in.WriteByte('/')
		}
		in.WriteByte(0)
	}
	// Exit 1 means nothing is ignored, 128 that dir isn't in a repository.
	code, out, _, err := Run(ctx, env, dir, []string{"git", "check-ignore", "--stdin", "-z"}, in.Bytes(), 30*time.Second)
	if err != nil || code != 0 {
		return nil
	}
	ignored := map[string]bool{}
	for p := range strings.SplitSeq(strings.TrimSuffix(string(out), "\x00"), "\x00") {
		ignored[strings.TrimSuffix(p, "/")] = true
	}
	return ignored
}

// Stat follows links. A missing path is Exists false, not an error.
func Stat(path string) (Info, error) {
	info, err := os.Stat(path)
	if errors.Is(err, fs.ErrNotExist) {
		return Info{}, nil
	}
	if err != nil {
		return Info{}, err
	}
	return Info{Exists: true, Dir: info.IsDir(), Size: info.Size(), Mtime: info.ModTime().UnixMilli()}, nil
}

// Read is up to max bytes of path (ReadMax when max is 0), its full size, and
// whether that was more.
func Read(path string, max int64) (data []byte, size int64, truncated bool, err error) {
	if max <= 0 {
		max = ReadMax
	}
	f, err := os.Open(path)
	if err != nil {
		return nil, 0, false, err
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil {
		return nil, 0, false, err
	}
	data, err = io.ReadAll(io.LimitReader(f, max))
	if err != nil {
		return nil, 0, false, err
	}
	return data, info.Size(), info.Size() > int64(len(data)), nil
}

// Write replaces path's content, or with create makes it and fails if it exists.
func Write(path string, data []byte, create bool) error {
	flag := os.O_WRONLY | os.O_CREATE | os.O_TRUNC
	if create {
		flag = os.O_WRONLY | os.O_CREATE | os.O_EXCL
	}
	f, err := os.OpenFile(path, flag, 0o644)
	if err != nil {
		return err
	}
	_, err = f.Write(data)
	if cerr := f.Close(); err == nil {
		err = cerr
	}
	return err
}

func Mkdir(path string, parents bool) error {
	if parents {
		return os.MkdirAll(path, 0o755)
	}
	return os.Mkdir(path, 0o755)
}

func Remove(path string, recursive bool) error {
	if recursive {
		return os.RemoveAll(path)
	}
	return os.Remove(path)
}
