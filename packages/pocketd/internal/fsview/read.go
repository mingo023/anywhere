package fsview

import (
	"bytes"
	"os"
	"path"
	"strings"
	"unicode/utf8"

	"pocketd/internal/hostop"
	"pocketd/internal/proto"
)

const (
	// TextMax is what fs.read sends of a text file when the phone names no max (preview.rs MAX_BYTES).
	TextMax = 512 << 10
	// ImageMax is the largest image fs.read sends (preview.rs MAX_IMAGE_BYTES).
	ImageMax = 20 << 20
	// binarySniff is how far a NUL makes a file binary.
	binarySniff = 8 << 10
)

// Read is the preview of the file at full, named rel in its tree, decided
// from its stat first as preview.rs classify does. Text over max is cut at a
// character boundary and marked truncated; text over hostop.ReadMax and images
// over ImageMax are tooLarge. SVG goes as text with its image mime.
func Read(full, rel string, max int) proto.FsRead {
	r := proto.FsRead{Path: rel}
	info, err := os.Stat(full)
	if err != nil || info.IsDir() || !info.Mode().IsRegular() {
		r.Kind = "unreadable"
		return r
	}
	r.Size, r.Mtime = info.Size(), info.ModTime().UnixMilli()
	mime, image := imageMime(rel)
	limit := int64(hostop.ReadMax)
	if image {
		limit = ImageMax
	}
	if r.Size > limit {
		r.Kind = "tooLarge"
		return r
	}
	if max <= 0 {
		max = TextMax
	}
	if image {
		max = ImageMax
	}
	data, _, truncated, err := hostop.Read(full, int64(max))
	if err != nil {
		r.Kind = "unreadable"
		return r
	}
	if image {
		r.Kind, r.Mime, r.Data = "image", mime, data
		return r
	}
	text, ok := decode(data, truncated)
	if !ok {
		r.Kind = "binary"
		return r
	}
	r.Kind, r.Text, r.Truncated, r.Language = "text", &text, truncated, Language(rel)
	if strings.EqualFold(path.Ext(rel), ".svg") {
		r.Mime = "image/svg+xml"
	}
	return r
}

// decode is text unless the first 8 KiB hold a NUL or the bytes aren't UTF-8
// (preview.rs decode). A cut read may end mid-character; that tail is dropped.
func decode(data []byte, cut bool) (string, bool) {
	if bytes.IndexByte(data[:min(len(data), binarySniff)], 0) >= 0 {
		return "", false
	}
	if cut {
		for i := 0; i < utf8.UTFMax && len(data) > 0; i++ {
			if utf8.Valid(data) {
				break
			}
			data = data[:len(data)-1]
		}
	}
	if !utf8.Valid(data) {
		return "", false
	}
	return string(data), true
}

// imageMime is the mime of an image fs.read sends as bytes (preview.rs
// image_format, less svg, which goes as text).
func imageMime(name string) (string, bool) {
	m, ok := map[string]string{
		".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".gif": "image/gif",
		".webp": "image/webp", ".bmp": "image/bmp", ".ico": "image/x-icon",
	}[strings.ToLower(path.Ext(name))]
	return m, ok
}

// Language is protocol.md's language id for a file name, "text" when it has none.
func Language(name string) string {
	switch strings.ToLower(path.Ext(name)) {
	case ".rs":
		return "rust"
	case ".ts", ".mts", ".cts":
		return "typescript"
	case ".tsx":
		return "tsx"
	case ".js", ".jsx", ".mjs", ".cjs":
		return "javascript"
	case ".py":
		return "python"
	case ".go":
		return "go"
	case ".swift":
		return "swift"
	case ".kt", ".kts":
		return "kotlin"
	case ".java":
		return "java"
	case ".json":
		return "json"
	case ".md", ".markdown":
		return "markdown"
	case ".toml":
		return "toml"
	case ".yml", ".yaml":
		return "yaml"
	case ".sh", ".zsh", ".bash":
		return "bash"
	case ".css":
		return "css"
	case ".html", ".htm":
		return "html"
	}
	return "text"
}
