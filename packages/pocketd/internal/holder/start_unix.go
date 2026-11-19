//go:build unix

package holder

import (
	"bytes"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"syscall"
	"time"
)

// Start runs `pocketd hold` for spec in its own session, so neither launchd
// nor a dying pocketd takes it down, and attaches to it. adopted, when set, is
// the pty of spec.AdoptPid, which the holder takes as fd 3.
func (s *Starter) Start(spec Spec, adopted *os.File) (*Client, error) {
	if err := os.MkdirAll(s.Dir, 0o700); err != nil {
		return nil, err
	}
	var name [4]byte
	rand.Read(name[:])
	spec.Sock = filepath.Join(s.Dir, hex.EncodeToString(name[:])+".sock")
	in, err := json.Marshal(spec)
	if err != nil {
		return nil, err
	}
	cmd := exec.Command(s.Exe, "hold")
	cmd.Stdin = bytes.NewReader(in)
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
	if adopted != nil {
		cmd.ExtraFiles = []*os.File{adopted}
	}
	out, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		return nil, err
	}
	var r struct {
		Pid   int    `json:"pid"`
		Error string `json:"error"`
	}
	err = json.NewDecoder(out).Decode(&r)
	// Reaped here while this pocketd lives; launchd or init reaps it after.
	go cmd.Wait()
	if err != nil {
		return nil, fmt.Errorf("holder didn't start: %w", err)
	}
	if r.Error != "" {
		return nil, errors.New(r.Error)
	}
	return Dial(spec.Sock, 5*time.Second)
}
