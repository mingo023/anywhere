package terminal

import (
	"bytes"
	"encoding/base64"
	"strings"
	"unicode"
)

// Report is one root record of the Program Status Protocol (OSC 7501).
type Report struct {
	State string // idle, working, blocked, done, error or clear
	Kind  string // permission, question or auth, with blocked
	App   string
	Msg   string
}

var oscStart = []byte("\x1b]7501;")

// queryReply tells a program that asked with "?" that this terminal reports status.
const queryReply = "\x1b]7501;?\x1b\\"

const (
	maxSequence = 4096 // from ESC ] through the terminator
	maxMsg      = 2048 // decoded
)

var states = map[string]bool{"idle": true, "working": true, "blocked": true, "done": true, "error": true, "clear": true}

// scanner finds OSC 7501 sequences in a byte stream that may split them anywhere.
type scanner struct{ partial []byte }

// feed returns the root reports in b, and whether b held the support query.
func (s *scanner) feed(b []byte) (reports []Report, query bool) {
	data := append(s.partial, b...)
	s.partial = nil
	for {
		i := bytes.Index(data, oscStart)
		if i < 0 {
			s.partial = splitPrefix(data)
			return
		}
		rest := data[i+len(oscStart):]
		end, size := terminator(rest)
		switch {
		case end == incomplete:
			if len(oscStart)+len(rest) <= maxSequence {
				s.partial = append(append([]byte(nil), oscStart...), rest...)
			}
			return
		case end == malformed:
			data = rest[len(rest)-size:]
		default:
			body := rest[:end]
			data = rest[end+size:]
			if len(oscStart)+end+size > maxSequence {
				continue
			}
			if string(body) == "?" {
				query = true
			} else if r, ok := parse(body); ok {
				reports = append(reports, r)
			}
		}
	}
}

const (
	incomplete = -1
	malformed  = -2
)

// terminator finds BEL or ESC \ . A lone ESC aborts the sequence: malformed,
// and size is how much of b is left, starting at that ESC.
func terminator(b []byte) (end, size int) {
	for i, c := range b {
		switch {
		case c == 0x07:
			return i, 1
		case c == 0x1b && i+1 == len(b):
			return incomplete, 0
		case c == 0x1b && b[i+1] == '\\':
			return i, 2
		case c == 0x1b:
			return malformed, len(b) - i
		}
	}
	return incomplete, 0
}

// splitPrefix keeps the tail of data that could be the start of oscStart.
func splitPrefix(data []byte) []byte {
	for n := min(len(oscStart)-1, len(data)); n > 0; n-- {
		if bytes.HasPrefix(oscStart, data[len(data)-n:]) {
			return append([]byte(nil), data[len(data)-n:]...)
		}
	}
	return nil
}

func parse(body []byte) (Report, bool) {
	var r Report
	var id string
	for _, pair := range strings.Split(string(body), ":") {
		k, v, ok := strings.Cut(pair, "=")
		if !ok {
			continue
		}
		switch k {
		case "state":
			r.State = v
		case "kind":
			r.Kind = v
		case "app":
			r.App = v
		case "id":
			id = v
		case "msg":
			raw, err := base64.RawStdEncoding.DecodeString(strings.TrimRight(v, "="))
			if err == nil && len(raw) <= maxMsg && !strings.ContainsFunc(string(raw), unicode.IsControl) {
				r.Msg = string(raw)
			}
		}
	}
	return r, states[r.State] && id == ""
}
