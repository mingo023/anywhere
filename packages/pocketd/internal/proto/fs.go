package proto

// CapFiles gates fs.list, fs.read, fs.search and attach.put.
const CapFiles = "fs.v1"

// CodeAttachInvalid: not an image type the desktop pastes, or over its size.
const CodeAttachInvalid = "attach_invalid"

const (
	FsSearchLimit = 500
	// AttachChunk is the most one attach.put carries, decoded.
	AttachChunk = 512 << 10
)

var fsVerbs = map[string]verbDecoder{
	"fs.list": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.get("dir", &m.Dir)
	},
	"fs.read": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.text("path", &m.Path) && f.optionalInt("max", &m.Max, 1, maxCount)
	},
	"fs.search": func(f fields, m *ClientMessage) bool {
		return f.text("tree", &m.Tree) && f.get("query", &m.Query) && f.limit(m, FsSearchLimit)
	},
	"attach.put": func(f fields, m *ClientMessage) bool {
		return f.get("uploadId", &m.UploadID) && ValidLocalID(m.UploadID) && f.text("name", &m.Name) &&
			f.integer("index", &m.Index, 0, maxCount) && f.bytes("data", &m.Data, AttachChunk) && f.get("last", &m.Last)
	},
}

// FsEntry's Mtime is unix ms. Git is a changed file's status letter, or the
// letter a folder's changes share.
type FsEntry struct {
	Name    string `json:"name"`
	Dir     bool   `json:"dir"`
	Link    bool   `json:"link,omitempty"`
	Size    int64  `json:"size"`
	Mtime   int64  `json:"mtime"`
	Ignored bool   `json:"ignored,omitempty"`
	Git     string `json:"git,omitempty"`
}

// FsList holds folders first, then files, each by name.
type FsList struct {
	Type    string    `json:"type"`
	ID      string    `json:"id"`
	Tree    string    `json:"tree"`
	Dir     string    `json:"dir"`
	Entries []FsEntry `json:"entries"`
}

func NewFsList(id, tree, dir string, entries []FsEntry) FsList {
	return FsList{"fs.list", id, tree, dir, orEmpty(entries)}
}

// FsRead.Kind is text, image, binary, tooLarge or unreadable. Text is set
// for text, Data (base64 on the wire) for an image.
type FsRead struct {
	Type      string  `json:"type"`
	ID        string  `json:"id"`
	Tree      string  `json:"tree"`
	Path      string  `json:"path"`
	Kind      string  `json:"kind"`
	Size      int64   `json:"size"`
	Mtime     int64   `json:"mtime"`
	Text      *string `json:"text,omitempty"`
	Truncated bool    `json:"truncated,omitempty"`
	Language  string  `json:"language,omitempty"`
	Mime      string  `json:"mime,omitempty"`
	Data      []byte  `json:"data,omitempty"`
}

func NewFsRead(r FsRead) FsRead {
	r.Type = "fs.read"
	return r
}

type FsSearch struct {
	Type  string   `json:"type"`
	ID    string   `json:"id"`
	Paths []string `json:"paths"`
}

func NewFsSearch(id string, paths []string) FsSearch {
	return FsSearch{"fs.search", id, orEmpty(paths)}
}

// AttachDone names where the upload landed, under <home>/attachments.
type AttachDone struct {
	Type     string `json:"type"`
	ID       string `json:"id"`
	UploadID string `json:"uploadId"`
	Path     string `json:"path"`
}

func NewAttachDone(id, uploadID, path string) AttachDone {
	return AttachDone{"attach.done", id, uploadID, path}
}
