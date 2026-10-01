package terminal

import (
	"errors"
	"strings"
)

const MaxPrompt = 64 << 10

var ErrPromptTooLarge = errors.New("prompt exceeds 64 KiB")

// Sanitize drops the bytes that let a prompt act as keys rather than text:
// C0 controls except newline and tab, ESC (so no control sequences) and DEL.
func Sanitize(text string) (string, error) {
	clean := strings.Map(func(r rune) rune {
		if r == '\n' || r == '\t' || r >= 0x20 && r != 0x7f {
			return r
		}
		return -1
	}, text)
	if len(clean) > MaxPrompt {
		return "", ErrPromptTooLarge
	}
	return clean, nil
}
