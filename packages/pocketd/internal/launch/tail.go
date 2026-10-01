package launch

import (
	"strings"
	"unicode/utf8"
)

const (
	tailLines = 20
	tailBytes = 4 << 10
)

// Tail is the end of a screen for an error's detail: the last 20 lines with
// trailing blanks dropped, cut to 4 KiB on a rune boundary.
func Tail(screen string) string {
	lines := strings.Split(screen, "\n")
	for n, l := range lines {
		lines[n] = strings.TrimRight(l, " ")
	}
	for len(lines) > 0 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	lines = lines[max(0, len(lines)-tailLines):]
	s := strings.Join(lines, "\n")
	if len(s) > tailBytes {
		s = s[len(s)-tailBytes:]
		for len(s) > 0 && !utf8.RuneStart(s[0]) {
			s = s[1:]
		}
	}
	return s
}
