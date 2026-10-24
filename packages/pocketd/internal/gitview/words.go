package gitview

import (
	"unicode"
	"unicode/utf8"
)

type token struct {
	at   int
	text string
}

// tokens are runs of letters, digits and _, and every other char alone (git.rs tokens).
func tokens(s string) []token {
	var out []token
	start := -1
	for i, c := range s {
		if unicode.IsLetter(c) || unicode.IsNumber(c) || c == '_' {
			if start < 0 {
				start = i
			}
			continue
		}
		if start >= 0 {
			out = append(out, token{start, s[start:i]})
			start = -1
		}
		out = append(out, token{i, s[i : i+utf8.RuneLen(c)]})
	}
	if start >= 0 {
		out = append(out, token{start, s[start:]})
	}
	return out
}

// Words are the byte ranges that differ between two versions of a line,
// compared word by word: removed from old, added in new (git.rs words).
func Words(old, new string) (del, add [][2]int) {
	a, b := tokens(old), tokens(new)
	ids := map[string]int{}
	intern := func(ts []token) []int {
		out := make([]int, len(ts))
		for i, t := range ts {
			id, ok := ids[t.text]
			if !ok {
				id = len(ids)
				ids[t.text] = id
			}
			out[i] = id
		}
		return out
	}
	ai, bi := intern(a), intern(b)
	gone, came := make([]bool, len(ai)), make([]bool, len(bi))
	histogram(ai, bi, 0, 0, gone, came)
	span := func(ts []token, lo, hi int) [2]int { return [2]int{ts[lo].at, ts[hi-1].at + len(ts[hi-1].text)} }
	for i, j := 0, 0; i < len(ai) || j < len(bi); {
		if i < len(ai) && j < len(bi) && !gone[i] && !came[j] {
			i++
			j++
			continue
		}
		si, sj := i, j
		for i < len(ai) && gone[i] {
			i++
		}
		for j < len(bi) && came[j] {
			j++
		}
		if i == si && j == sj {
			// Unmatched only on one side past the other's end.
			if i < len(ai) {
				gone[i] = true
			} else {
				came[j] = true
			}
			continue
		}
		if i > si {
			del = append(del, span(a, si, i))
		}
		if j > sj {
			add = append(add, span(b, sj, j))
		}
	}
	return del, add
}

// maxChain is imara's and git's limit on how often a token may recur and still anchor a split.
const maxChain = 63

// histogram marks what a and b don't share, as git's and imara's Histogram diff:
// split on the longest common run of the rarest tokens, then recurse either side.
func histogram(a, b []int, ao, bo int, gone, came []bool) {
	for len(a) > 0 && len(b) > 0 && a[0] == b[0] {
		a, b, ao, bo = a[1:], b[1:], ao+1, bo+1
	}
	for len(a) > 0 && len(b) > 0 && a[len(a)-1] == b[len(b)-1] {
		a, b = a[:len(a)-1], b[:len(b)-1]
	}
	if len(a) == 0 || len(b) == 0 {
		for i := range a {
			gone[ao+i] = true
		}
		for j := range b {
			came[bo+j] = true
		}
		return
	}
	at := map[int][]int{}
	for i, t := range a {
		at[t] = append(at[t], i)
	}
	bestLen, bestCount, as, bs := 0, maxChain+1, 0, 0
	for j := 0; j < len(b); {
		next := j + 1
		for _, i := range at[b[j]] {
			if len(at[b[j]]) > maxChain {
				break
			}
			sa, sb, ea, eb := i, j, i+1, j+1
			rc := len(at[a[i]])
			for sa > 0 && sb > 0 && a[sa-1] == b[sb-1] {
				sa, sb = sa-1, sb-1
				rc = min(rc, len(at[a[sa]]))
			}
			for ea < len(a) && eb < len(b) && a[ea] == b[eb] {
				rc = min(rc, len(at[a[ea]]))
				ea, eb = ea+1, eb+1
			}
			next = max(next, eb)
			if ea-sa > bestLen || rc < bestCount {
				bestLen, bestCount, as, bs = ea-sa, rc, sa, sb
			}
		}
		j = next
	}
	if bestLen == 0 {
		lcs(a, b, ao, bo, gone, came)
		return
	}
	histogram(a[:as], b[:bs], ao, bo, gone, came)
	histogram(a[as+bestLen:], b[bs+bestLen:], ao+as+bestLen, bo+bs+bestLen, gone, came)
}

// lcs is the fallback when no token is rare enough: an exact LCS, or all
// changed when the two are too long to compare that way.
func lcs(a, b []int, ao, bo int, gone, came []bool) {
	if len(a)*len(b) > 1<<20 {
		for i := range a {
			gone[ao+i] = true
		}
		for j := range b {
			came[bo+j] = true
		}
		return
	}
	w := len(b) + 1
	t := make([]int, (len(a)+1)*w)
	for i := len(a) - 1; i >= 0; i-- {
		for j := len(b) - 1; j >= 0; j-- {
			if a[i] == b[j] {
				t[i*w+j] = t[(i+1)*w+j+1] + 1
			} else {
				t[i*w+j] = max(t[(i+1)*w+j], t[i*w+j+1])
			}
		}
	}
	i, j := 0, 0
	for i < len(a) && j < len(b) {
		switch {
		case a[i] == b[j]:
			i, j = i+1, j+1
		case t[(i+1)*w+j] >= t[i*w+j+1]:
			gone[ao+i] = true
			i++
		default:
			came[bo+j] = true
			j++
		}
	}
	for ; i < len(a); i++ {
		gone[ao+i] = true
	}
	for ; j < len(b); j++ {
		came[bo+j] = true
	}
}
