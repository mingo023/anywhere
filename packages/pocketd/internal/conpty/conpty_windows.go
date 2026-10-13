// Package conpty runs a program in a Windows pseudoconsole, in a job that
// takes its whole process tree down with it.
package conpty

import (
	"errors"
	"os"
	"sync"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

type Console struct {
	hpc     windows.Handle
	job     windows.Handle
	in, out *os.File
	pid     int
	hangup  sync.Once
	closed  sync.Once
}

// Start runs argv in a cols×rows pseudoconsole. A Read that returns io.EOF
// means the program exited and its output is drained.
func Start(argv []string, dir string, env []string, cols, rows int) (_ *Console, err error) {
	var inR, inW, outR, outW windows.Handle
	if err := windows.CreatePipe(&inR, &inW, nil, 0); err != nil {
		return nil, err
	}
	if err := windows.CreatePipe(&outR, &outW, nil, 0); err != nil {
		windows.CloseHandle(inR)
		windows.CloseHandle(inW)
		return nil, err
	}
	c := &Console{in: os.NewFile(uintptr(inW), "conpty-in"), out: os.NewFile(uintptr(outR), "conpty-out")}
	defer func() {
		if err != nil {
			c.Close()
		}
	}()
	// The pseudoconsole holds its own copies of its pipe ends.
	err = windows.CreatePseudoConsole(coord(cols, rows), inR, outW, 0, &c.hpc)
	windows.CloseHandle(inR)
	windows.CloseHandle(outW)
	if err != nil {
		return nil, err
	}
	if c.job, err = killOnClose(); err != nil {
		return nil, err
	}
	pi, err := create(c.hpc, argv, dir, env)
	if err != nil {
		return nil, err
	}
	defer windows.CloseHandle(pi.Thread)
	if err := windows.AssignProcessToJobObject(c.job, pi.Process); err != nil {
		windows.TerminateProcess(pi.Process, 1)
		windows.CloseHandle(pi.Process)
		return nil, err
	}
	if _, err := windows.ResumeThread(pi.Thread); err != nil {
		windows.TerminateProcess(pi.Process, 1)
		windows.CloseHandle(pi.Process)
		return nil, err
	}
	c.pid = int(pi.ProcessId)
	// The output pipe stays open while the pseudoconsole lives, even after
	// the program exits; closing it then lets Read reach EOF.
	go func() {
		windows.WaitForSingleObject(pi.Process, windows.INFINITE)
		windows.CloseHandle(pi.Process)
		c.Hangup()
	}()
	return c, nil
}

func coord(cols, rows int) windows.Coord { return windows.Coord{X: int16(cols), Y: int16(rows)} }

func killOnClose() (windows.Handle, error) {
	job, err := windows.CreateJobObject(nil, nil)
	if err != nil {
		return 0, err
	}
	info := windows.JOBOBJECT_EXTENDED_LIMIT_INFORMATION{}
	info.BasicLimitInformation.LimitFlags = windows.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
	if _, err := windows.SetInformationJobObject(job, windows.JobObjectExtendedLimitInformation, uintptr(unsafe.Pointer(&info)), uint32(unsafe.Sizeof(info))); err != nil {
		windows.CloseHandle(job)
		return 0, err
	}
	return job, nil
}

func create(hpc windows.Handle, argv []string, dir string, env []string) (*windows.ProcessInformation, error) {
	attrs, err := windows.NewProcThreadAttributeList(1)
	if err != nil {
		return nil, err
	}
	defer attrs.Delete()
	if err := attrs.Update(windows.PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, *(*unsafe.Pointer)(unsafe.Pointer(&hpc)), unsafe.Sizeof(hpc)); err != nil {
		return nil, err
	}
	si := &windows.StartupInfoEx{ProcThreadAttributeList: attrs.List()}
	si.Cb = uint32(unsafe.Sizeof(*si))
	cmdline, err := windows.UTF16PtrFromString(windows.ComposeCommandLine(argv))
	if err != nil {
		return nil, err
	}
	var cwd *uint16
	if dir != "" {
		if cwd, err = windows.UTF16PtrFromString(dir); err != nil {
			return nil, err
		}
	}
	block, err := EnvBlock(env)
	if err != nil {
		return nil, err
	}
	pi := &windows.ProcessInformation{}
	flags := uint32(windows.EXTENDED_STARTUPINFO_PRESENT | windows.CREATE_UNICODE_ENVIRONMENT | windows.CREATE_SUSPENDED)
	if err := windows.CreateProcess(nil, cmdline, nil, nil, false, flags, &block[0], cwd, &si.StartupInfo, pi); err != nil {
		return nil, err
	}
	return pi, nil
}

// EnvBlock is env as CreateProcess takes it: each "k=v" NUL-terminated, then one more NUL.
func EnvBlock(env []string) ([]uint16, error) {
	var block []uint16
	for _, kv := range env {
		u, err := windows.UTF16FromString(kv)
		if err != nil {
			return nil, err
		}
		block = append(block, u...)
	}
	return append(block, 0), nil
}

func (c *Console) Pid() int { return c.pid }

func (c *Console) Read(b []byte) (int, error) { return c.out.Read(b) }

func (c *Console) Write(b []byte) (int, error) { return c.in.Write(b) }

func (c *Console) Resize(cols, rows int) error {
	return windows.ResizePseudoConsole(c.hpc, coord(cols, rows))
}

// SetReadDeadline fails: anonymous pipes can't be polled.
func (c *Console) SetReadDeadline(time.Time) error { return os.ErrNoDeadline }

// Hangup closes the pseudoconsole, which sends its programs CTRL_CLOSE_EVENT
// and ends output.
func (c *Console) Hangup() {
	c.hangup.Do(func() {
		if c.hpc != 0 {
			windows.ClosePseudoConsole(c.hpc)
		}
	})
}

// Kill ends every process the program started.
func (c *Console) Kill() error {
	if c.job == 0 {
		return errors.New("conpty: no job")
	}
	return windows.TerminateJobObject(c.job, 1)
}

// Close hangs up, then closes the pipes and the job, which kills what is left.
func (c *Console) Close() error {
	c.Hangup()
	c.closed.Do(func() {
		c.in.Close()
		c.out.Close()
		if c.job != 0 {
			windows.CloseHandle(c.job)
		}
	})
	return nil
}
