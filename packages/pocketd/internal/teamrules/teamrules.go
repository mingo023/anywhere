// Package teamrules finds the team rules pocketd puts before a lead's prompt (team.Wrap), so the chat shows the
// user only what they typed. It imports nothing of pocketd's, so the transcript readers can use it.
package teamrules

import "strings"

const (
	open = "<anywhere_team>\n"
	end  = "\n</anywhere_team>\n"
)

// Strip is text without the team rules that start it; text as it is when none do.
func Strip(text string) string {
	if !strings.HasPrefix(text, open) {
		return text
	}
	if _, after, ok := strings.Cut(text, end); ok {
		return strings.TrimLeft(after, "\n")
	}
	return text
}
