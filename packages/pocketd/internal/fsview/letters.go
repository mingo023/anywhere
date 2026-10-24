package fsview

import (
	"path"
	"strings"
)

// Letters are git's letters for one tree's changes, keyed by path relative to
// the tree: M A D R U, and ? for untracked. Folders carry the letter their
// changes share, else M (explorer.rs folder_status).
type Letters struct {
	files, folders map[string]string
	// untracked are folders git lists whole, as "dir/", so all inside is ?.
	untracked map[string]bool
}

// File is the letter of the changed file at rel, or "".
func (l Letters) File(rel string) string {
	if s, ok := l.files[rel]; ok {
		return s
	}
	return l.inUntracked(path.Dir(rel))
}

// Folder is the letter the changes inside the folder at rel share, or "".
func (l Letters) Folder(rel string) string {
	if s, ok := l.folders[rel]; ok {
		return s
	}
	return l.inUntracked(rel)
}

func (l Letters) inUntracked(dir string) string {
	for ; dir != "." && dir != "/" && dir != ""; dir = path.Dir(dir) {
		if l.untracked[dir] {
			return "?"
		}
	}
	return ""
}

// ParseStatus reads `git status --porcelain=v2 -z` into the letters of the
// tree at prefix (git rev-parse --show-prefix: "" or "dir/"), whose paths are
// relative to the repository root. Changes outside the tree are left out.
func ParseStatus(out []byte, prefix string) Letters {
	l := Letters{files: map[string]string{}, folders: map[string]string{}, untracked: map[string]bool{}}
	records := strings.Split(string(out), "\x00")
	for i := 0; i < len(records); i++ {
		r := records[i]
		if r == "" {
			continue
		}
		var letter, p string
		switch r[0] {
		case '1':
			if f := strings.SplitN(r, " ", 9); len(f) == 9 {
				letter, p = ordinary(f[1]), f[8]
			}
		case '2':
			if f := strings.SplitN(r, " ", 10); len(f) == 10 {
				letter, p = "R", f[9]
			}
			i++ // the original path follows
		case 'u':
			if f := strings.SplitN(r, " ", 11); len(f) == 11 {
				letter, p = "U", f[10]
			}
		case '?':
			letter, p = "?", strings.TrimPrefix(r, "? ")
		}
		rel, ok := strings.CutPrefix(p, prefix)
		if letter == "" || !ok || rel == "" {
			continue
		}
		dir := path.Dir(rel)
		if folder, whole := strings.CutSuffix(rel, "/"); whole {
			l.untracked[folder] = true
			dir = folder
		} else {
			l.files[rel] = letter
		}
		for ; dir != "."; dir = path.Dir(dir) {
			if had, seen := l.folders[dir]; !seen {
				l.folders[dir] = letter
			} else if had != letter {
				l.folders[dir] = "M"
			}
		}
	}
	return l
}

// ordinary is the letter of a change against HEAD from its XY, as the
// desktop's `git diff HEAD --name-status` reads it.
func ordinary(xy string) string {
	switch {
	case len(xy) != 2:
		return ""
	case xy[0] == 'A' && xy[1] == 'D':
		return ""
	case xy[0] == 'A':
		return "A"
	case xy[0] == 'D' || xy[1] == 'D':
		return "D"
	}
	return "M"
}
