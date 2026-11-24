package proto

import (
	"testing"
)

func TestTheRegistryMessagesDecodeTheirFields(t *testing.T) {
	m, err := DecodeClient([]byte(`{"type":"project.add","id":"1","path":"/p","repo":{"name":"App","color":7,"launch":null,"icon":{"emoji":"A"},"hidden":["codex"]}}`))
	if err != nil || m.Path != "/p" || m.Repo == nil || m.Repo.Name != "App" || m.Repo.Color != 7 || m.Repo.Launch != nil || string(m.Repo.Icon) != `{"emoji":"A"}` || m.Repo.Hidden[0] != "codex" {
		t.Fatalf("%+v %v", m, err)
	}
	if m, err = DecodeClient([]byte(`{"type":"project.add","id":"1","path":"/p"}`)); err != nil || m.Repo != nil {
		t.Fatalf("%+v %v", m, err)
	}
	if m, err = DecodeClient([]byte(`{"type":"worktree.track","id":"1","path":"/w","tracked":false}`)); err != nil || m.Path != "/w" || m.Tracked {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient([]byte(`{"type":"settings.shared","id":"1","settings":{"git":{"remote":"fork","commit_all":false},"access":"edits","worktree_root":"/wt","external_worktrees":true}}`))
	s := m.Settings
	if err != nil || s == nil || s.Git.Remote != "fork" || s.Git.CommitsAll() || s.Access != "edits" || s.WorktreeRoot != "/wt" || !s.ExternalWorktrees {
		t.Fatalf("%+v %v", m, err)
	}
}
