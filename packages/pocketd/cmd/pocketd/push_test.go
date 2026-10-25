package main

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/push"
)

func TestPushStatusListsPhonesAndLastSendsWithoutSecrets(t *testing.T) {
	home := t.TempDir()
	store, _ := push.Open(push.Path(home))
	now := time.Unix(1761300000, 0)
	store.Register("dev-1234567890", push.Registration{Key: bytes.Repeat([]byte{7}, 32), Env: "production",
		Alert: &proto.PushHandle{Handle: "h", Secret: "the-secret", ExpiresAt: now.Add(30 * 24 * time.Hour).Unix()},
		Start: &proto.PushHandle{Handle: "s", Secret: "the-secret", ExpiresAt: 1},
		Prefs: proto.PushPrefs{NeedsYou: true, Failed: true}}, now)
	store.Note(push.Sent{At: now.Add(-2 * time.Minute).UnixMilli(), Device: "dev-1234567890", Kind: push.NeedsYou, Priority: 10, Outcome: "sent"})
	var out bytes.Buffer
	if err := pushStatus(home, nil, &out, now); err != nil {
		t.Fatal(err)
	}
	text := out.String()
	for _, want := range []string{"dev-1234", "until 2025-11-23", "starts on Needs you", "needs you, failed", "2 min ago", "needsYou", "p10", "sent"} {
		if !strings.Contains(text, want) {
			t.Errorf("no %q in\n%s", want, text)
		}
	}
	out.Reset()
	pushStatus(home, []string{"--json"}, &out, now)
	var parsed struct{ Phones, Sends []map[string]any }
	if err := json.Unmarshal(out.Bytes(), &parsed); err != nil || len(parsed.Phones) != 1 || len(parsed.Sends) != 1 {
		t.Fatalf("%v %s", err, out.Bytes())
	}
	if strings.Contains(out.String()+text, "the-secret") || strings.Contains(out.String(), `"key"`) {
		t.Fatal("printed a secret or key")
	}
}
