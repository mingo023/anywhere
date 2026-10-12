package ops

import (
	"context"
	"os"
	"path/filepath"
	"time"

	"pocketd/internal/hostop"
)

// hostOp answers exec and fs.* for a desktop whose project lives on this host.
func (s *Server) hostOp(ctx context.Context, m Msg) Msg {
	paths := []string{m.Path}
	switch m.Op {
	case "exec":
		paths = []string{m.Cwd}
	case "fs.rename":
		paths = []string{m.From, m.To}
	}
	for _, p := range paths {
		if !filepath.IsAbs(p) {
			return Msg{Ev: "error", ID: m.ID, Error: "paths must be absolute"}
		}
	}
	r := Msg{Ev: m.Op, ID: m.ID}
	var err error
	switch m.Op {
	case "exec":
		r.Code, r.Stdout, r.Stderr, err = hostop.Run(ctx, s.loginEnv(), m.Cwd, m.Argv, m.Stdin, time.Duration(m.TimeoutMs)*time.Millisecond)
	case "fs.list":
		r.Entries, err = hostop.List(ctx, s.loginEnv(), m.Path)
	case "fs.stat":
		var info hostop.Info
		info, err = hostop.Stat(m.Path)
		r.Exists, r.Dir, r.Size, r.Mtime = info.Exists, info.Dir, info.Size, info.Mtime
	case "fs.read":
		r.Data, r.Size, r.Truncated, err = hostop.Read(m.Path, m.Max)
	case "fs.write":
		err = hostop.Write(m.Path, m.Data, m.Create)
	case "fs.mkdir":
		err = hostop.Mkdir(m.Path, m.Parents)
	case "fs.rename":
		err = os.Rename(m.From, m.To)
	case "fs.remove":
		err = hostop.Remove(m.Path, m.Recursive)
	case "fs.trash":
		r.Trashed, err = hostop.Trash(m.Path, s.TrashDir)
	}
	if err != nil {
		return Msg{Ev: "error", ID: m.ID, Error: err.Error()}
	}
	return r
}

func (s *Server) loginEnv() []string {
	if s.LoginEnv == nil {
		return os.Environ()
	}
	return s.LoginEnv()
}
