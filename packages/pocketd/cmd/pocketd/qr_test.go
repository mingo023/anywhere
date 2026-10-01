package main

import (
	"regexp"
	"strings"
	"testing"
	"unicode/utf8"
)

func TestTheQRCodeHasAQuietZoneAndSquareModules(t *testing.T) {
	art, err := qrText("codingpocket://pair?v=1&h=100.77.122.82%3A4517&c=q3xYq3xYq3xYq3xYq3xYq3&n=Mac")
	if err != nil {
		t.Fatal(err)
	}
	rows := strings.Split(strings.TrimSuffix(regexp.MustCompile("\x1b\\[[0-9;]*m").ReplaceAllString(art, ""), "\n"), "\n")
	width := utf8.RuneCountInString(rows[0])
	if len(rows) != (width+1)/2 {
		t.Fatalf("%d rows for %d columns", len(rows), width)
	}
	for _, r := range rows {
		if utf8.RuneCountInString(r) != width {
			t.Fatalf("ragged row %q", r)
		}
	}
	if strings.TrimSpace(rows[0]+rows[1]) != "" {
		t.Fatalf("no quiet zone: %q", rows[:2])
	}
	// Finder pattern: modules (4,4) and (4,5) dark, (5,4) dark, (5,5) light.
	if got := []rune(rows[2])[4:6]; string(got) != "█▀" {
		t.Fatalf("finder corner %q", string(got))
	}
}
