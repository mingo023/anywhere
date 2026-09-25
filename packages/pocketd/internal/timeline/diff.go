package timeline

import (
	"strconv"
	"strings"

	"pocketd/internal/proto"
)

const syncWindow = 8

func textLines(text string) []string {
	if text == "" {
		return nil
	}
	lines := strings.Split(strings.ReplaceAll(text, "\r\n", "\n"), "\n")
	if lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	return lines
}

func capRunes(text string, n int) string {
	r := []rune(text)
	if len(r) <= n {
		return text
	}
	return string(r[:n]) + "…"
}

// findSync returns the nearest matching pair within the window, so a small
// edit stays a small hunk.
func findSync(old, new []string, i, j int) (int, int, bool) {
	for di := 0; di <= syncWindow; di++ {
		for dj := 0; dj <= syncWindow; dj++ {
			if di == 0 && dj == 0 {
				continue
			}
			oi, nj := i+di, j+dj
			if oi < len(old) && nj < len(new) && old[oi] == new[nj] {
				return oi, nj, true
			}
		}
	}
	return 0, 0, false
}

func greedyDiff(old, new []string) []proto.DiffLine {
	var out []proto.DiffLine
	i, j := 0, 0
	for i < len(old) || j < len(new) {
		if i < len(old) && j < len(new) && old[i] == new[j] {
			out = append(out, proto.DiffLine{Number: j + 1, Kind: "context", Text: old[i]})
			i, j = i+1, j+1
			continue
		}
		if si, sj, ok := findSync(old, new, i, j); ok {
			for ; i < si; i++ {
				out = append(out, proto.DiffLine{Number: i + 1, Kind: "del", Text: old[i]})
			}
			for ; j < sj; j++ {
				out = append(out, proto.DiffLine{Number: j + 1, Kind: "add", Text: new[j]})
			}
			continue
		}
		if i < len(old) {
			out = append(out, proto.DiffLine{Number: i + 1, Kind: "del", Text: old[i]})
			i++
		} else {
			out = append(out, proto.DiffLine{Number: j + 1, Kind: "add", Text: new[j]})
			j++
		}
	}
	return out
}

func FileDiff(oldText, newText string) *proto.FileDiff {
	contentOnly := oldText == ""
	var hunks []proto.DiffLine
	if contentOnly {
		for n, text := range textLines(newText) {
			hunks = append(hunks, proto.DiffLine{Number: n + 1, Kind: "add", Text: text})
		}
	} else {
		hunks = greedyDiff(textLines(oldText), textLines(newText))
	}
	return preview(hunks, contentOnly)
}

// preview keeps one line of context before the first change.
func preview(hunks []proto.DiffLine, contentOnly bool) *proto.FileDiff {
	d := &proto.FileDiff{Lines: []proto.DiffLine{}, ContentOnly: contentOnly}
	start := 0
	for n, l := range hunks {
		if l.Kind != "context" {
			start = max(0, n-1)
			break
		}
	}
	for n, l := range hunks {
		switch l.Kind {
		case "add":
			d.Additions++
		case "del":
			d.Deletions++
		}
		if n >= start && n < start+proto.DiffPreviewLines {
			l.Text = capRunes(l.Text, proto.DiffLineChars)
			d.Lines = append(d.Lines, l)
		}
	}
	return d
}

// ParseUnified reads the first file of a unified diff, as Codex reports it.
// Hunk line counts, not prefixes, mark where a hunk ends: a deleted "--"
// line reads as "---" inside one.
func ParseUnified(diff string) *proto.FileDiff {
	var hunks []proto.DiffLine
	oldN, newN, oldLeft, newLeft := 0, 0, 0, 0
	inFile := false
	for _, line := range textLines(diff) {
		if oldLeft > 0 || newLeft > 0 {
			switch {
			case strings.HasPrefix(line, "+"):
				hunks = append(hunks, proto.DiffLine{Number: newN, Kind: "add", Text: line[1:]})
				newN, newLeft = newN+1, newLeft-1
			case strings.HasPrefix(line, "-"):
				hunks = append(hunks, proto.DiffLine{Number: oldN, Kind: "del", Text: line[1:]})
				oldN, oldLeft = oldN+1, oldLeft-1
			case strings.HasPrefix(line, `\`):
			default:
				hunks = append(hunks, proto.DiffLine{Number: newN, Kind: "context", Text: strings.TrimPrefix(line, " ")})
				oldN, newN, oldLeft, newLeft = oldN+1, newN+1, oldLeft-1, newLeft-1
			}
			continue
		}
		switch {
		case strings.HasPrefix(line, "@@"):
			oldN, oldLeft, newN, newLeft = hunkHeader(line)
			inFile = true
		case inFile && (strings.HasPrefix(line, "diff ") || strings.HasPrefix(line, "--- ")):
			return preview(hunks, false)
		}
	}
	return preview(hunks, false)
}

// hunkHeader parses "@@ -3,4 +3,5 @@"; a missing count means 1.
func hunkHeader(header string) (oldStart, oldCount, newStart, newCount int) {
	f := strings.Fields(header)
	if len(f) < 3 {
		return 1, 0, 1, 0
	}
	span := func(s string) (int, int) {
		start, count, found := strings.Cut(s[1:], ",")
		n, _ := strconv.Atoi(start)
		if !found {
			return n, 1
		}
		c, _ := strconv.Atoi(count)
		return n, c
	}
	oldStart, oldCount = span(f[1])
	newStart, newCount = span(f[2])
	return
}
