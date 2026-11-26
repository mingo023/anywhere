//go:build unix

package e2e

import (
	"bytes"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"testing"
)

func TestAPortSetByPocketdConfigIsTheListenPortAfterARestartAndConfigJSONIsUntouched(t *testing.T) {
	h := Start(t)
	path := filepath.Join(h.Home, "config.json")
	original, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	old, port := h.Port, freePort(t)
	if out, code := h.Pocketd("config", "set", "port", strconv.Itoa(port)); code != 0 {
		t.Fatalf("exit %d: %s", code, out)
	}
	h.Restart()
	h.eventually("the new port to be served", func() bool {
		c, err := net.Dial("tcp", net.JoinHostPort("127.0.0.1", strconv.Itoa(port)))
		if err == nil {
			c.Close()
		}
		return err == nil
	})
	if c, err := net.Dial("tcp", net.JoinHostPort("127.0.0.1", strconv.Itoa(old))); err == nil {
		c.Close()
		t.Fatalf("the old port %d is still served", old)
	}
	if now, _ := os.ReadFile(path); !bytes.Equal(now, original) {
		t.Fatalf("config.json became %s", now)
	}
	if got := configRow(t, h, "port"); got != strconv.Itoa(port) {
		t.Fatalf("pocketd.db holds port %s", got)
	}
}
