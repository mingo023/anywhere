package proto

import (
	"encoding/base64"
	"encoding/json"
)

// CapPush gates the push.* verbs.
const CapPush = "push.v1"

// PushHandle is what the relay's /v1/register returned to the phone; ExpiresAt is unix seconds.
type PushHandle struct {
	Handle    string `json:"handle"`
	Secret    string `json:"secret"`
	ExpiresAt int64  `json:"expiresAt"`
}

// PushPrefs say which alerts a device wants; Details shows the ask's verb and subject on the Live Activity.
type PushPrefs struct {
	NeedsYou    bool `json:"needsYou"`
	Done        bool `json:"done"`
	Failed      bool `json:"failed"`
	Automations bool `json:"automations"`
	Details     bool `json:"details"`
}

func decodeHandle(raw json.RawMessage, dst **PushHandle) bool {
	var h PushHandle
	if !strictInto(raw, &h, []string{"handle", "secret", "expiresAt"}) || h.Handle == "" || h.Secret == "" {
		return false
	}
	*dst = &h
	return true
}

func (f fields) optionalHandle(key string, dst **PushHandle) bool {
	return !f.has(key) || decodeHandle(f[key], dst)
}

func decodePrefs(raw json.RawMessage, dst **PushPrefs) bool {
	var p PushPrefs
	if !strictInto(raw, &p, []string{"needsYou", "done", "failed", "automations", "details"}) {
		return false
	}
	*dst = &p
	return true
}

// validSealKey is base64 of a 32-byte AES-256-GCM key.
func validSealKey(key string) bool {
	b, err := base64.StdEncoding.DecodeString(key)
	return err == nil && len(b) == 32
}

var pushVerbs = map[string]verbDecoder{
	"push.register": func(f fields, m *ClientMessage) bool {
		return f.get("key", &m.Key) && validSealKey(m.Key) &&
			f.get("env", &m.Env) && (m.Env == "development" || m.Env == "production") &&
			decodeHandle(f["alert"], &m.Alert) && f.optionalHandle("start", &m.Start) &&
			decodePrefs(f["prefs"], &m.Prefs) && f.optionalBool("frequent", &m.Frequent)
	},
	"push.activity": func(f fields, m *ClientMessage) bool {
		return f.text("activityId", &m.ActivityID) && f.optionalHandle("handle", &m.Handle) && f.optionalBool("ended", &m.Ended)
	},
	"push.prefs":      func(f fields, m *ClientMessage) bool { return decodePrefs(f["prefs"], &m.Prefs) },
	"push.unregister": func(fields, *ClientMessage) bool { return true },
	"push.test":       func(fields, *ClientMessage) bool { return true },
}
