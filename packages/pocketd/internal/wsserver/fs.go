package wsserver

import (
	"context"
	"errors"
	"io/fs"
	"slices"
	"sync"

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

// filesConn is a conn's state for this area.
type filesConn struct {
	reads   chan struct{}
	mu      sync.Mutex
	uploads map[string]*fsview.Upload
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
	for id, u := range c.files.uploads {
		u.Abort()
		delete(c.files.uploads, id)
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

// background answers m off the read loop, a few at a time.
func (c *conn) background(m proto.ClientMessage, serve func() (any, error)) error {
	c.helloFiles()
	select {
	case c.files.reads <- struct{}{}:
	case <-c.ctx.Done():
		return nil
	}
	go func() {
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
		f.uploads = map[string]*fsview.Upload{}
	}
	u := f.uploads[m.UploadID]
	if m.Index == 0 {
		if u != nil {
			u.Abort()
			delete(f.uploads, m.UploadID)
		}
		if len(f.uploads) >= fsUploads {
			return "", &fsview.Refusal{Code: proto.CodeAttachInvalid, Message: "Too many images uploading at once"}
		}
		var err error
		if u, err = c.s.Files.StartUpload(m.UploadID, m.Name); err != nil {
			return "", err
		}
		f.uploads[m.UploadID] = u
	}
	if u == nil {
		return "", &fsview.Refusal{Code: proto.CodeAttachInvalid, Message: "That upload hasn't started"}
	}
	if err := u.Put(m.Index, m.Data); err != nil {
		u.Abort()
		delete(f.uploads, m.UploadID)
		return "", err
	}
	if !m.Last {
		return "", nil
	}
	delete(f.uploads, m.UploadID)
	return u.Finish()
}
