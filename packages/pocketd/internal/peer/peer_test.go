package peer

import (
	"net"
	"os"
	"path/filepath"
	"testing"
)

func TestPIDNamesTheProcessOnTheOtherEnd(t *testing.T) {
	dir, err := os.MkdirTemp("/tmp", "peer")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	ln, err := net.Listen("unix", filepath.Join(dir, "s.sock"))
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	go func() {
		c, _ := net.Dial("unix", filepath.Join(dir, "s.sock"))
		defer c.Close()
		c.Read(make([]byte, 1))
	}()
	c, err := ln.Accept()
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	if pid, err := PID(c.(*net.UnixConn)); err != nil || pid != os.Getpid() {
		t.Fatalf("%d %v", pid, err)
	}
}
