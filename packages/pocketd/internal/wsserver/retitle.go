package wsserver

import (
	"path/filepath"
	"strings"

	"pocketd/internal/terminal"
)

// retitle names the Claude conversations in the Terminals that match after a
// rename, off the conn's loop, since it writes to their transcripts.
func (c *conn) retitle(match func(terminal.Info) bool, title string) {
	if c.s.Retitle != nil {
		go c.s.Retitle(match, title)
	}
}

// inside is whether path is dir or under it.
func inside(path, dir string) bool {
	rel, err := filepath.Rel(dir, path)
	return err == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}
