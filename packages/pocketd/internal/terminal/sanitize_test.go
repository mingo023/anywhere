package terminal

import (
	"errors"
	"strings"
	"testing"
)

func TestSanitizeStripsC0EscAndDelButKeepsNewlineAndTab(t *testing.T) {
	for in, want := range map[string]string{
		"plain":                      "plain",
		"two\nlines\tand tab":        "two\nlines\tand tab",
		"\x1b[201~y\r":               "[201~y",
		"a\x00b\x03c\x7fd":           "abcd",
		"héllo 世界":                   "héllo 世界",
		"\x1b]52;c;cGF3bmVk\x07done": "]52;c;cGF3bmVkdone",
	} {
		if got, err := Sanitize(in); err != nil || got != want {
			t.Errorf("%q: %q %v, want %q", in, got, err, want)
		}
	}
}

func TestAPromptOver64KiBIsRefused(t *testing.T) {
	if _, err := Sanitize(strings.Repeat("a", MaxPrompt)); err != nil {
		t.Fatalf("64 KiB exactly: %v", err)
	}
	if _, err := Sanitize(strings.Repeat("a", MaxPrompt+1)); !errors.Is(err, ErrPromptTooLarge) {
		t.Fatalf("got %v", err)
	}
}

func TestPromptTypesOnlyTheSanitizedText(t *testing.T) {
	s := spawn(t, NewManager(), `printf 'ready\n'; read x; echo "got:$x"; sleep 5`)
	waitScreen(t, s, "ready")
	if err := s.Prompt("a\x03b\x1b[Dc"); err != nil {
		t.Fatal(err)
	}
	waitScreen(t, s, "got:ab[Dc")
}
