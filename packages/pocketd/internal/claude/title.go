package claude

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

// Retitle names the conversation in the transcript at path, as /rename in it
// would. A running claude reads the last title in its transcript before it
// writes its own again, so it takes the name too. A transcript claude hasn't
// started yet is left alone, as is a blank title, which would clear the name.
func Retitle(path, title string) error {
	title = strings.TrimSpace(title)
	if title == "" {
		return nil
	}
	f, err := os.OpenFile(path, os.O_WRONLY|os.O_APPEND, 0)
	if err != nil {
		return err
	}
	defer f.Close()
	// Claude matches the line as written, so no \u-escaped < > & and one write per line.
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	enc.Encode(struct {
		Type        string `json:"type"`
		CustomTitle string `json:"customTitle"`
		SessionID   string `json:"sessionId"`
	}{"custom-title", title, strings.TrimSuffix(filepath.Base(path), ".jsonl")})
	_, err = f.Write(buf.Bytes())
	return err
}
