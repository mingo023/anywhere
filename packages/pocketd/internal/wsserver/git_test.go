package wsserver

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/devices"
	"pocketd/internal/gitview"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

var phoneDevice = peer.FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes})

func gitEnv() []string {
	return append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1",
		"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t")
}

func gitIn(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = gitEnv()
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
}

// withGitProject serves git.v1 over a registered project holding one commit of src/main.rs.
func withGitProject(t *testing.T) (string, func(*Server)) {
	dir, _ := filepath.EvalSymlinks(t.TempDir())
	repo := filepath.Join(dir, "pocket")
	os.MkdirAll(filepath.Join(repo, "src"), 0o755)
	gitIn(t, repo, "init", "-q", "-b", "main")
	os.WriteFile(filepath.Join(repo, "src", "main.rs"), []byte("fn main() {\n    println!(\"hi\");\n}\n"), 0o644)
	gitIn(t, repo, "add", ".")
	gitIn(t, repo, "commit", "-qm", "Root")
	reg := filepath.Join(dir, "desktop.json")
	raw, _ := json.Marshal(map[string]any{"projects": []string{repo}, "repos": map[string]any{}})
	os.WriteFile(reg, raw, 0o600)
	return repo, func(s *Server) {
		s.Registry = registry.New(reg)
		s.Git = &gitview.Service{Env: gitEnv, Every: 20 * time.Millisecond}
	}
}

func q(s string) string { raw, _ := json.Marshal(s); return string(raw) }

// reply skips pushes until the message answering id.
func (p *phone) reply(id string) map[string]any {
	p.t.Helper()
	for {
		m := p.recv()
		if m["id"] == id {
			return m
		}
	}
}

func TestAPhoneReadsATreesStatusDiffLogAndCommit(t *testing.T) {
	repo, opt := withGitProject(t)
	_, _, p := local(t, phoneDevice, opt)
	helloWith(p, proto.CapGit)
	os.WriteFile(filepath.Join(repo, "src", "main.rs"), []byte("fn main() {\n    println!(\"hello\");\n}\n"), 0o644)

	p.send(`{"type":"git.status","id":"s","tree":` + q(repo) + `}`)
	st := p.reply("s")
	files := st["files"].([]any)
	if st["type"] != "git.status" || st["branch"] != "main" || len(files) != 1 || files[0].(map[string]any)["path"] != "src/main.rs" {
		t.Fatalf("%v", st)
	}
	p.send(`{"type":"git.diff","id":"d","tree":` + q(repo) + `,"path":"src/main.rs","at":{"kind":"working"}}`)
	d := p.reply("d")
	lines := d["lines"].([]any)
	if d["type"] != "git.diff" || d["language"] != "rust" || len(lines) != 5 || lines[3].(map[string]any)["w"] == nil {
		t.Fatalf("%v", d)
	}
	p.send(`{"type":"git.log","id":"l","tree":` + q(repo) + `}`)
	l := p.reply("l")
	commits := l["commits"].([]any)
	if l["type"] != "git.log" || len(commits) != 1 || l["more"] != false {
		t.Fatalf("%v", l)
	}
	sha := commits[0].(map[string]any)["sha"].(string)
	p.send(`{"type":"git.commit","id":"c","tree":` + q(repo) + `,"sha":"` + sha + `"}`)
	if c := p.reply("c"); c["type"] != "git.commit" || c["subject"] != "Root" || len(c["files"].([]any)) != 1 {
		t.Fatalf("%v", c)
	}
}

func TestAPhoneStagesCommitsAndHearsTheTreeMove(t *testing.T) {
	repo, opt := withGitProject(t)
	_, _, p := local(t, phoneDevice, opt)
	helloWith(p, proto.CapGit)
	p.send(`{"type":"git.watch","id":"w","trees":[` + q(repo) + `]}`)
	if m := p.reply("w"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	time.Sleep(60 * time.Millisecond)
	os.WriteFile(filepath.Join(repo, "notes.md"), []byte("n\n"), 0o644)
	for m := p.recv(); m["type"] != "git.changed" || m["tree"] != repo; m = p.recv() {
	}
	p.send(`{"type":"git.stage","id":"st","tree":` + q(repo) + `,"paths":["notes.md"],"staged":true}`)
	if m := p.reply("st"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"git.commit.create","id":"cc","tree":` + q(repo) + `,"message":"Add notes"}`)
	if m := p.reply("cc"); m["type"] != "git.committed" || len(m["sha"].(string)) != 40 {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"git.push","id":"p","tree":` + q(repo) + `}`)
	if m := p.reply("p"); m["code"] != proto.CodeGitFailed || m["message"] == "" || m["detail"] == nil {
		t.Fatalf("a push without a remote: %v", m)
	}
	p.send(`{"type":"git.watch","id":"w2","trees":[]}`)
	if m := p.reply("w2"); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
}

func TestGitVerbsStayInsideRegisteredTrees(t *testing.T) {
	repo, opt := withGitProject(t)
	_, _, p := local(t, phoneDevice, opt)
	helloWith(p, proto.CapGit)
	for _, c := range []struct{ raw, code string }{
		{`{"type":"git.status","id":"1","tree":"/etc"}`, proto.CodeNotATree},
		{`{"type":"git.status","id":"2","tree":` + q(repo+"/../pocket") + `}`, proto.CodeNotATree},
		{`{"type":"git.watch","id":"3","trees":["relative"]}`, proto.CodeNotATree},
		{`{"type":"git.diff","id":"4","tree":` + q(repo) + `,"path":"../secret","at":{"kind":"working"}}`, proto.CodeOutsideTree},
		{`{"type":"git.stage","id":"5","tree":` + q(repo) + `,"paths":["a","/etc/hosts"],"staged":true}`, proto.CodeOutsideTree},
	} {
		p.send(c.raw)
		if m := p.recv(); m["type"] != "error" || m["code"] != c.code {
			t.Errorf("%s: %v", c.raw, m)
		}
	}
}
