package wsserver

import (
	"context"
	"errors"
	"io/fs"
	"slices"
	"sync"
	"time"

	"pocketd/internal/fsview"
	"pocketd/internal/proto"
)

// FileService serves the fs.v1 verbs; fsview.Service is the real one.
type FileService interface {
	List(ctx context.Context, tree, dir string) ([]proto.FsEntry, error)
	Read(tree, path string, max int) (proto.FsRead, error)
	Search(ctx context.Context, tree, query string, limit int) ([]string, error)
	StartUpload(id, name string) (*fsview.Upload, error)
}

const (
	// fsReads is how many list, read and search requests one conn runs at once;
	// the next waits, so a phone can't pile up 20 MB reads.
	fsReads = 4
	// fsUploads is how many uploads one conn may have open.
	fsUploads       = 4
	fsSearchDefault = 50
)

// uploadIdle is how long an upload waits for its next chunk before it is
// dropped with its part file; tests shorten it.
var uploadIdle = 60 * time.Second

// filesConn is a conn's state for this area.
type filesConn struct {
	reads   chan struct{}
	mu      sync.Mutex
	uploads map[string]*upload
}

// upload is one open upload, dropped once idle fires.
type upload struct {
	*fsview.Upload
	idle *time.Timer
}

// drop forgets id's upload, the part file with it unless it landed.
func (f *filesConn) drop(id string, landed bool) {
	u := f.uploads[id]
	if u == nil {
		return
	}
	u.idle.Stop()
	if !landed {
		u.Abort()
	}
	delete(f.uploads, id)
}

func (c *conn) helloFiles() {
	if c.files.reads == nil {
		c.files.reads = make(chan struct{}, fsReads)
	}
}

// closeFiles drops uploads the socket didn't finish.
func (c *conn) closeFiles() {
	c.files.mu.Lock()
	defer c.files.mu.Unlock()
	for id := range c.files.uploads {
		c.files.drop(id, false)
	}
}

// filesVerb serves its cap's verbs; without the cap they are unknown.
func (c *conn) filesVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapFiles) {
		return proto.ErrMalformed
	}
	if c.s.Files == nil {
		return c.notImplemented(m)
	}
	switch m.Type {
	case "fs.list":
		return c.fsList(m)
	case "fs.read":
		return c.fsRead(m)
	case "fs.search":
		return c.fsSearch(m)
	case "attach.put":
		return c.attachPut(m)
	}
	return proto.ErrMalformed
}

// background answers m off the read loop, a few at a time; the rest wait
// there too, so the conn's other messages don't.
func (c *conn) background(m proto.ClientMessage, serve func() (any, error)) error {
	c.helloFiles()
	go func() {
		select {
		case c.files.reads <- struct{}{}:
		case <-c.ctx.Done():
			return
		}
		defer func() { <-c.files.reads }()
		reply, err := serve()
		if err != nil {
			c.send(fsError(m.ID, err))
			return
		}
		c.send(reply)
	}()
	return nil
}

func fsError(id string, err error) proto.Error {
	if r, ok := errors.AsType[*fsview.Refusal](err); ok {
		return proto.NewErrorCode(id, r.Code, r.Message)
	}
	if errors.Is(err, fs.ErrNotExist) {
		return proto.NewError(id, "That folder is gone")
	}
	if errors.Is(err, fs.ErrPermission) {
		return proto.NewError(id, "Your Mac won't let pocketd read that folder")
	}
	return proto.NewError(id, err.Error())
}

func (c *conn) fsList(m proto.ClientMessage) error {
	return c.background(m, func() (any, error) {
		entries, err := c.s.Files.List(c.ctx, m.Tree, m.Dir)
		return proto.NewFsList(m.ID, m.Tree, m.Dir, entries), err
	})
}

func (c *conn) fsRead(m proto.ClientMessage) error {
	return c.background(m, func() (any, error) {
		r, err := c.s.Files.Read(m.Tree, m.Path, m.Max)
		r.ID = m.ID
		return proto.NewFsRead(r), err
	})
}

func (c *conn) fsSearch(m proto.ClientMessage) error {
	limit := fsSearchDefault
	if m.Limit != nil {
		limit = int(*m.Limit)
	}
	return c.background(m, func() (any, error) {
		paths, err := c.s.Files.Search(c.ctx, m.Tree, m.Query, limit)
		return proto.NewFsSearch(m.ID, paths), err
	})
}

// attachPut takes chunks in order on the read loop; index 0 starts (or
// restarts) the upload, last lands it.
func (c *conn) attachPut(m proto.ClientMessage) error {
	path, err := c.putChunk(m)
	switch {
	case err != nil:
		c.send(fsError(m.ID, err))
	case m.Last:
		c.send(proto.NewAttachDone(m.ID, m.UploadID, path))
	default:
		c.send(proto.NewAck(m.ID))
	}
	return nil
}

func (c *conn) putChunk(m proto.ClientMessage) (string, error) {
	f := &c.files
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.uploads == nil {
		f.uploads = map[string]*upload{}
	}
	id := m.UploadID
	if m.Index == 0 {
		f.drop(id, false)
		if len(f.uploads) >= fsUploads {
			return "", &fsview.Refusal{Code: proto.CodeAttachInvalid, Message: "Too many images uploading at once"}
		}
		started, err := c.s.Files.StartUpload(id, m.Name)
		if err != nil {
			return "", err
		}
		u := &upload{Upload: started}
		// A phone that stops sending part-way, without closing its socket, leaves nothing behind either.
		u.idle = time.AfterFunc(uploadIdle, func() {
			f.mu.Lock()
			defer f.mu.Unlock()
			if f.uploads[id] == u {
				f.drop(id, false)
			}
		})
		f.uploads[id] = u
	}
	u := f.uploads[id]
	if u == nil {
		return "", &fsview.Refusal{Code: proto.CodeAttachInvalid, Message: "That upload hasn't started"}
	}
	if err := u.Put(m.Index, m.Data); err != nil {
		f.drop(id, false)
		return "", err
	}
	if !m.Last {
		u.idle.Reset(uploadIdle)
		return "", nil
	}
	f.drop(id, true)
	return u.Finish()
}
