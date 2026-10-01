package launch

import (
	"fmt"
	"strings"
	"testing"
	"unicode/utf8"
)

func TestTailKeepsTheLastTwentyLines(t *testing.T) {
	var b strings.Builder
	for n := 1; n <= 30; n++ {
		fmt.Fprintf(&b, "line %d   \n", n)
	}
	b.WriteString("\n\n   \n")
	got := strings.Split(Tail(b.String()), "\n")
	if len(got) != 20 || got[0] != "line 11" || got[19] != "line 30" {
		t.Fatalf("got %q", got)
	}
}

func TestTailIsAtMostFourKiBAndValidUTF8(t *testing.T) {
	got := Tail(strings.Repeat("é", 5000))
	if len(got) > 4096 || !utf8.ValidString(got) {
		t.Fatalf("len %d valid %v", len(got), utf8.ValidString(got))
	}
}
