package envdir

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLookup(t *testing.T) {
	home, _ := os.UserHomeDir()
	cases := []struct {
		env  []string
		want string
	}{
		{[]string{"X_HOME=/a", "X_HOME=/b"}, "/b"},
		{[]string{"X_HOME=/a", "X_HOME="}, filepath.Join(home, ".x")},
		{[]string{"OTHER_X_HOME=/a"}, filepath.Join(home, ".x")},
	}
	for _, c := range cases {
		if got := Lookup(c.env, "X_HOME", ".x"); got != c.want {
			t.Errorf("%v: got %q want %q", c.env, got, c.want)
		}
	}
}
