//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"testing"
)

func TestANewWorktreeRunsTheSetupOfWhicheverFolderHoldsTheReposConfig(t *testing.T) {
	setup := func(who string) string {
		return `{"setup":["printf 'RAN:%s:%s:%s:%s\\n' ` + who + ` \"$ANYWHERE_WORKSPACE_NAME\" \"$ZOES_WORKSPACE_NAME\" \"$(basename \"$ZOES_ROOT_PATH\")\""]}`
	}
	for name, c := range map[string]struct {
		dirs map[string]string
		want string
	}{
		"only .anywhere": {map[string]string{".anywhere": "anywhere"}, "RAN:anywhere:calm-otter:calm-otter:repo"},
		"only .zoes":     {map[string]string{".zoes": "zoes"}, "RAN:zoes:calm-otter:calm-otter:repo"},
		"both":           {map[string]string{".anywhere": "anywhere", ".zoes": "zoes"}, "RAN:zoes:calm-otter:calm-otter:repo"},
	} {
		t.Run(name, func(t *testing.T) {
			h := Start(t, launchReady(""))
			for dir, who := range c.dirs {
				if err := os.MkdirAll(filepath.Join(h.Repo, dir), 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(filepath.Join(h.Repo, dir, "config.json"), []byte(setup(who)), 0o644); err != nil {
					t.Fatal(err)
				}
			}
			o := h.Owner()
			create(o, "r1", inRepo(h, map[string]any{"checkout": map[string]any{"new": map[string]any{"name": "calm-otter"}}, "prompt": "hello"}))
			made := reply(o, "agent.creating", "error")
			if made.Type != "agent.creating" || !made.Setup {
				t.Fatalf("got %s", made.Raw)
			}
			h.WaitScreen(made.TerminalID, c.want)
		})
	}
}
