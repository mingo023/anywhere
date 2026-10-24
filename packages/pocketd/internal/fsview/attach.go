package fsview

import (
	"bytes"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/proto"
)

// AttachMax is the most one upload may hold.
const AttachMax = 20 << 20

// attachTypes are the image types a chat takes, by extension, each with the
// first bytes its file starts with.
var attachTypes = map[string]func([]byte) bool{
	".png":  func(b []byte) bool { return bytes.HasPrefix(b, []byte("\x89PNG\r\n\x1a\n")) },
	".jpg":  isJPEG,
	".jpeg": isJPEG,
	".gif": func(b []byte) bool {
		return bytes.HasPrefix(b, []byte("GIF87a")) || bytes.HasPrefix(b, []byte("GIF89a"))
	},
	".webp": func(b []byte) bool { return len(b) >= 12 && string(b[:4]) == "RIFF" && string(b[8:12]) == "WEBP" },
	".heic": func(b []byte) bool {
		return len(b) >= 12 && string(b[4:8]) == "ftyp" && slices.Contains([]string{"heic", "heix", "hevc", "heim", "heis", "mif1", "msf1"}, string(b[8:12]))
	},
}

func isJPEG(b []byte) bool { return bytes.HasPrefix(b, []byte{0xff, 0xd8, 0xff}) }

func invalid(format string, a ...any) error {
	return &Refusal{proto.CodeAttachInvalid, fmt.Sprintf(format, a...)}
}

// Upload is one image arriving in chunks, written owner-only to a part file
// beside where it lands, as the desktop saves a pasted image.
type Upload struct {
	ID, ext, dir string
	next         int
	size         int
	file         *os.File
}

// StartUpload checks the first chunk's name and makes dir (0700) for it.
func StartUpload(dir, id, name string) (*Upload, error) {
	ext := strings.ToLower(path.Ext(name))
	if attachTypes[ext] == nil {
		return nil, invalid("Only PNG, JPEG, GIF, WebP and HEIC images can be attached")
	}
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(filepath.Join(dir, id+ext+".part"), os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0o600)
	if err != nil {
		return nil, err
	}
	return &Upload{ID: id, ext: ext, dir: dir, file: f}, nil
}

// Put appends chunk index, which must come next; the first must start as
// its type does.
func (u *Upload) Put(index int, data []byte) error {
	switch {
	case index != u.next:
		return invalid("Chunk %d came where %d was due", index, u.next)
	case u.size+len(data) > AttachMax:
		return invalid("Images over 20 MB can't be attached")
	case index == 0 && !attachTypes[u.ext](data):
		return invalid("That file isn't the image its name says")
	}
	if _, err := u.file.Write(data); err != nil {
		return err
	}
	u.next++
	u.size += len(data)
	return nil
}

// Finish closes the part file and moves it to <dir>/<id>.<ext>.
func (u *Upload) Finish() (string, error) {
	if err := u.file.Close(); err != nil {
		os.Remove(u.file.Name())
		return "", err
	}
	final := filepath.Join(u.dir, u.ID+u.ext)
	if err := os.Rename(u.file.Name(), final); err != nil {
		os.Remove(u.file.Name())
		return "", err
	}
	return final, nil
}

// StartUpload starts an upload into the Service's attachments folder.
func (s *Service) StartUpload(id, name string) (*Upload, error) {
	return StartUpload(s.Attachments, id, name)
}

// Abort drops what arrived.
func (u *Upload) Abort() {
	u.file.Close()
	os.Remove(u.file.Name())
}
