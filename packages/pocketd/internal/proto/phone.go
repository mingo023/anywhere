package proto

import (
	"encoding/base64"
	"encoding/json"
	"maps"
	"math"
	"slices"
)

const (
	CodeNotImplemented = "not_implemented"
	// CodeNotATree: the tree isn't a registered project, worktree or Local.
	CodeNotATree = "not_a_tree"
	// CodeOutsideTree: a relative path escapes its tree.
	CodeOutsideTree = "outside_tree"
)

// maxCount bounds counts and offsets so they survive a JavaScript number.
const maxCount = math.MaxInt32

// verbDecoder reads one phone verb's fields into m; false is malformed.
type verbDecoder = func(f fields, m *ClientMessage) bool

func phoneAreas() []map[string]verbDecoder {
	return []map[string]verbDecoder{terminalVerbs, workspaceVerbs, gitVerbs, ghVerbs, fsVerbs, pushVerbs}
}

func phoneVerb(t string) verbDecoder {
	for _, verbs := range phoneAreas() {
		if d, ok := verbs[t]; ok {
			return d
		}
	}
	return nil
}

// PhoneVerbs are the verbs of protocol.md's caps, sorted.
func PhoneVerbs() []string {
	var all []string
	for _, verbs := range phoneAreas() {
		all = append(all, slices.Collect(maps.Keys(verbs))...)
	}
	slices.Sort(all)
	return all
}

// ConfigValues are the values config.set takes per key; nil takes any non-empty value.
var ConfigValues = map[string][]string{
	"phone.maxAccess": PhoneAccesses,
	"phone.terminals": {"on", "off"},
	"push.relay":      nil,
}

func validConfig(key, value string) bool {
	values, ok := ConfigValues[key]
	return ok && (values == nil && value != "" || slices.Contains(values, value))
}

func (f fields) text(key string, dst *string) bool { return f.get(key, dst) && *dst != "" }

func (f fields) optionalBool(key string, dst *bool) bool { return !f.has(key) || f.get(key, dst) }

// optionalFlag leaves dst nil when key is absent.
func (f fields) optionalFlag(key string, dst **bool) bool {
	if !f.has(key) {
		return true
	}
	*dst = new(bool)
	return f.get(key, *dst)
}

// integer reads a whole number in lo..hi.
func (f fields) integer(key string, dst *int, lo, hi int) bool {
	var v float64
	if !f.get(key, &v) || v != math.Trunc(v) || v < float64(lo) || v > float64(hi) {
		return false
	}
	*dst = int(v)
	return true
}

func (f fields) optionalInt(key string, dst *int, lo, hi int) bool {
	return !f.has(key) || f.integer(key, dst, lo, hi)
}

// limit reads an optional whole "limit" in 1..hi into m.Limit.
func (f fields) limit(m *ClientMessage, hi int) bool {
	if !f.has("limit") {
		return true
	}
	var n int
	if !f.integer("limit", &n, 1, hi) {
		return false
	}
	m.Limit = new(float64(n))
	return true
}

// bytes reads standard base64 of at most max decoded bytes.
func (f fields) bytes(key string, dst *[]byte, max int) bool {
	var s string
	if !f.get(key, &s) {
		return false
	}
	b, err := base64.StdEncoding.DecodeString(s)
	if err != nil || len(b) > max {
		return false
	}
	*dst = b
	return true
}

// paths is a non-empty list of non-empty paths.
func (f fields) paths(key string, dst *[]string) bool {
	return f.stringList(key, dst) && len(*dst) > 0 && !slices.Contains(*dst, "")
}

func (f fields) requestID(m *ClientMessage) bool {
	return f.get("requestId", &m.RequestID) && m.RequestID != "" && len(m.RequestID) <= 64
}

// strictInto decodes an object with exactly-matched keys, no nulls, and every required key.
func strictInto(raw json.RawMessage, dst any, required []string, allowed ...string) bool {
	obj, ok := strictObject(raw, append(allowed, required...)...)
	if !ok {
		return false
	}
	for _, k := range required {
		if _, has := obj[k]; !has {
			return false
		}
	}
	return json.Unmarshal(raw, dst) == nil
}
