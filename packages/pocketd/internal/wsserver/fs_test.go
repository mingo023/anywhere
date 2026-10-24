package wsserver

import (
	"encoding/base64"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"pocketd/internal/devices"
	"pocketd/internal/fsview"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

// withFiles serves fs.v1 over a registered git project holding a.rs; it
// returns the project's path and where uploads land.
func withFiles(t *testing.T) (func(*Server), string, string) {
	base, _ := filepath.EvalSymlinks(t.TempDir())
	root := filepath.Join(base, "app")
	os.MkdirAll(filepath.Join(root, "src"), 0o755)
	os.WriteFile(filepath.Join(root, "src", "a.rs"), []byte("fn a() {}\n"), 0o644)
	if out, err := exec.Command("git", "init", "-q", root).CombinedOutput(); err != nil {
		t.Fatal(err, string(out))
	}
	desktop := filepath.Join(base, "desktop.json")
	raw, _ := json.Marshal(registry.File{Projects: []string{root}})
	os.WriteFile(desktop, raw, 0o644)
	attachments := filepath.Join(base, "home", "attachments")
	files := &fsview.Service{Registry: registry.New(desktop), Env: os.Environ, Attachments: attachments}
	return func(s *Server) { s.Files = files }, root, attachments
}

func TestAPhoneListsReadsAndSearchesATree(t *testing.T) {
	files, root, _ := withFiles(t)
	_, _, p := local(t, peer.FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes}), files)
	helloWith(p, proto.CapFiles)

	p.send(`{"type":"fs.list","id":"l","tree":"` + root + `","dir":""}`)
	m := p.recv()
	entries, _ := m["entries"].([]any)
	if m["type"] != "fs.list" || m["id"] != "l" || m["tree"] != root || len(entries) != 1 || entries[0].(map[string]any)["git"] != "?" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"fs.read","id":"r","tree":"` + root + `","path":"src/a.rs"}`)
	if m := p.recv(); m["type"] != "fs.read" || m["id"] != "r" || m["kind"] != "text" || m["text"] != "fn a() {}\n" || m["language"] != "rust" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"fs.search","id":"s","tree":"` + root + `","query":"ars"}`)
	if m := p.recv(); m["type"] != "fs.search" || m["id"] != "s" || len(m["paths"].([]any)) != 1 {
		t.Fatalf("%v", m)
	}
}

func TestAPhoneCantReachOutsideARegisteredTree(t *testing.T) {
	files, root, _ := withFiles(t)
	_, _, p := local(t, peer.OwnerOf(1), files)
	helloWith(p, proto.CapFiles)
	for raw, code := range map[string]string{
		`{"type":"fs.list","id":"1","tree":"/etc","dir":""}`:                           proto.CodeNotATree,
		`{"type":"fs.list","id":"2","tree":"` + root + `","dir":".."}`:                 proto.CodeOutsideTree,
		`{"type":"fs.read","id":"3","tree":"` + root + `","path":"../desktop.json"}`:   proto.CodeOutsideTree,
		`{"type":"fs.search","id":"4","tree":"` + filepath.Dir(root) + `","query":""}`: proto.CodeNotATree,
	} {
		p.send(raw)
		if m := p.recv(); m["type"] != "error" || m["code"] != code {
			t.Errorf("%s: %v", raw, m)
		}
	}
}

func chunk(id string, index int, data []byte, last bool) string {
	raw, _ := json.Marshal(map[string]any{"type": "attach.put", "id": id, "uploadId": "u1", "name": "shot.png", "index": index,
		"data": base64.StdEncoding.EncodeToString(data), "last": last})
	return string(raw)
}

func TestAnImageUploadsInChunksAndLandsUnderAttachments(t *testing.T) {
	files, _, attachments := withFiles(t)
	_, _, p := local(t, peer.FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes}), files)
	helloWith(p, proto.CapFiles)
	image := []byte("\x89PNG\r\n\x1a\nIHDR and the rest")
	p.send(chunk("c0", 0, image[:12], false))
	if m := p.recv(); m["type"] != "ack" || m["id"] != "c0" {
		t.Fatalf("%v", m)
	}
	p.send(chunk("c1", 1, image[12:], true))
	m := p.recv()
	want := filepath.Join(attachments, "u1.png")
	if m["type"] != "attach.done" || m["id"] != "c1" || m["uploadId"] != "u1" || m["path"] != want {
		t.Fatalf("%v", m)
	}
	if got, _ := os.ReadFile(want); string(got) != string(image) {
		t.Fatalf("%q", got)
	}
}

func TestAnUploadThatIsntAnImageOrSkipsAChunkIsInvalid(t *testing.T) {
	files, _, _ := withFiles(t)
	_, _, p := local(t, peer.OwnerOf(1), files)
	helloWith(p, proto.CapFiles)
	for _, raw := range []string{chunk("a", 0, []byte("plain text"), true), chunk("b", 3, []byte("x"), false)} {
		p.send(raw)
		if m := p.recv(); m["code"] != proto.CodeAttachInvalid {
			t.Errorf("%s: %v", raw, m)
		}
	}
}

func TestAnObservingPeerMayBrowseButNotUpload(t *testing.T) {
	files, _, _ := withFiles(t)
	_, _, p := local(t, peer.FromDevice(devices.Device{ID: "d1", Scopes: []peer.Scope{peer.Observe}}), files)
	helloWith(p, proto.CapFiles)
	p.send(chunk("a", 0, []byte("\x89PNG\r\n\x1a\n"), true))
	if m := p.recv(); m["code"] != proto.CodeScopeDenied {
		t.Fatalf("%v", m)
	}
}
