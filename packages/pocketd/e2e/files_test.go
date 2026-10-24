//go:build unix

package e2e

import (
	"encoding/base64"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/proto"
)

// fsReply is a message read for its fs.v1 fields.
func fsReply(t *testing.T, m Message) map[string]any {
	t.Helper()
	var out map[string]any
	if err := json.Unmarshal([]byte(m.Raw), &out); err != nil {
		t.Fatal(err)
	}
	return out
}

func TestAPairedPhoneBrowsesSearchesAndAttachesThroughARealPocketd(t *testing.T) {
	h := Start(t, launchReady(""))
	os.MkdirAll(filepath.Join(h.Repo, "src"), 0o755)
	os.WriteFile(filepath.Join(h.Repo, "src", "main.go"), []byte("package main\n"), 0o644)
	os.WriteFile(filepath.Join(h.Repo, ".gitignore"), []byte("build/\n"), 0o644)
	os.MkdirAll(filepath.Join(h.Repo, "build"), 0o755)
	p := h.Paired(proto.CapFiles)
	byID := func(id string) func(Message) bool { return func(m Message) bool { return m.ID == id } }

	p.Send(map[string]any{"type": "fs.list", "id": "l", "tree": h.Repo, "dir": ""})
	list := fsReply(t, p.WaitFor("fs.list", byID("l")))
	entries, _ := list["entries"].([]any)
	if list["type"] != "fs.list" || len(entries) != 3 {
		t.Fatalf("%v", list)
	}
	first := entries[0].(map[string]any)
	if first["name"] != "build" || first["ignored"] != true || entries[1].(map[string]any)["git"] != "?" {
		t.Fatalf("%v", entries)
	}

	p.Send(map[string]any{"type": "fs.read", "id": "r", "tree": h.Repo, "path": "src/main.go"})
	if r := fsReply(t, p.WaitFor("fs.read", byID("r"))); r["kind"] != "text" || r["text"] != "package main\n" || r["language"] != "go" {
		t.Fatalf("%v", r)
	}

	p.Send(map[string]any{"type": "fs.search", "id": "s", "tree": h.Repo, "query": "mgo"})
	if s := fsReply(t, p.WaitFor("fs.search", byID("s"))); len(s["paths"].([]any)) != 1 || s["paths"].([]any)[0] != "src/main.go" {
		t.Fatalf("%v", s)
	}

	p.Send(map[string]any{"type": "fs.read", "id": "x", "tree": h.Repo, "path": "../desktop.json"})
	if e := p.WaitFor("error", byID("x")); e.Code != proto.CodeOutsideTree {
		t.Fatalf("%+v", e)
	}

	image := []byte("\x89PNG\r\n\x1a\nthe rest")
	p.Send(map[string]any{"type": "attach.put", "id": "a", "uploadId": "shot-1", "name": "IMG_0001.PNG", "index": 0,
		"data": base64.StdEncoding.EncodeToString(image), "last": true})
	done := fsReply(t, p.WaitFor("attach.done", byID("a")))
	want := filepath.Join(h.Home, "attachments", "shot-1.png")
	if done["type"] != "attach.done" || done["path"] != want {
		t.Fatalf("%v", done)
	}
	if got, err := os.ReadFile(want); err != nil || string(got) != string(image) {
		t.Fatal(err, got)
	}
}
