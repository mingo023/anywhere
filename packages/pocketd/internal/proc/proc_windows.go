package proc

import (
	"errors"
	"os"
	"slices"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

// entries maps each live pid to its parent's, as Toolhelp reports it.
func entries() (map[int]int, error) {
	snap, err := windows.CreateToolhelp32Snapshot(windows.TH32CS_SNAPPROCESS, 0)
	if err != nil {
		return nil, err
	}
	defer windows.CloseHandle(snap)
	parents := map[int]int{}
	e := windows.ProcessEntry32{Size: uint32(unsafe.Sizeof(windows.ProcessEntry32{}))}
	for err = windows.Process32First(snap, &e); err == nil; err = windows.Process32Next(snap, &e) {
		parents[int(e.ProcessID)] = int(e.ParentProcessID)
	}
	if !errors.Is(err, windows.ERROR_NO_MORE_FILES) {
		return nil, err
	}
	return parents, nil
}

// created is pid's start time; a pid Windows reused starts after its children.
func created(pid int) (int64, error) {
	h, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, uint32(pid))
	if err != nil {
		return 0, err
	}
	defer windows.CloseHandle(h)
	var c, x, k, u windows.Filetime
	if err := windows.GetProcessTimes(h, &c, &x, &k, &u); err != nil {
		return 0, err
	}
	return c.Nanoseconds(), nil
}

// Parent fails when the parent is gone or its pid was reused: Windows keeps
// a dead parent's pid in the child's entry.
func Parent(pid int) (int, error) {
	parents, err := entries()
	if err != nil {
		return 0, err
	}
	ppid, ok := parents[pid]
	if !ok {
		return 0, errors.New("proc: no such process")
	}
	if ppid == 0 {
		return 0, nil
	}
	if _, ok := parents[ppid]; !ok {
		return 0, errors.New("proc: parent gone")
	}
	child, err := created(pid)
	if err != nil {
		return 0, err
	}
	if parent, err := created(ppid); err != nil || parent > child {
		return 0, errors.New("proc: parent gone")
	}
	return ppid, nil
}

// Members is root and its live descendants, root first. Windows has no
// process groups, so a Terminal's "group" is its whole tree.
func Members(root int) ([]int, error) {
	parents, err := entries()
	if err != nil {
		return nil, err
	}
	if _, ok := parents[root]; !ok {
		return nil, nil
	}
	out := []int{root}
	for i := 0; i < len(out); i++ {
		for pid, ppid := range parents {
			if ppid == out[i] && pid != ppid && !slices.Contains(out, pid) {
				if c, err := created(pid); err == nil {
					if p, err := created(ppid); err == nil && p <= c {
						out = append(out, pid)
					}
				}
			}
		}
	}
	slices.Sort(out[1:])
	return out, nil
}

// Read gets pid's command line from its PEB. Env stays nil: nothing on
// Windows reads it.
func Read(pid int) (Proc, error) {
	h, err := windows.OpenProcess(windows.PROCESS_QUERY_INFORMATION|windows.PROCESS_VM_READ, false, uint32(pid))
	if err != nil {
		return Proc{}, err
	}
	defer windows.CloseHandle(h)
	line, err := commandLine(h)
	if err != nil {
		return Proc{}, err
	}
	return Proc{Pid: pid, Argv: SplitCommandLine(line)}, nil
}

// Offsets in the 64-bit PEB and RTL_USER_PROCESS_PARAMETERS. Reading them
// into x/sys's structs would put another process's addresses in Go pointers.
const (
	pebParams         = 0x20
	paramsCommandLine = 0x70
)

func commandLine(h windows.Handle) (string, error) {
	var info windows.PROCESS_BASIC_INFORMATION
	if err := windows.NtQueryInformationProcess(h, windows.ProcessBasicInformation, unsafe.Pointer(&info), uint32(unsafe.Sizeof(info)), nil); err != nil {
		return "", err
	}
	read := func(addr uintptr, buf []byte) error {
		return windows.ReadProcessMemory(h, addr, &buf[0], uintptr(len(buf)), nil)
	}
	var word [8]byte
	if err := read(uintptr(unsafe.Pointer(info.PebBaseAddress))+pebParams, word[:]); err != nil {
		return "", err
	}
	params := *(*uintptr)(unsafe.Pointer(&word))
	var us [16]byte
	if err := read(params+paramsCommandLine, us[:]); err != nil {
		return "", err
	}
	n := *(*uint16)(unsafe.Pointer(&us[0]))
	buf := *(*uintptr)(unsafe.Pointer(&us[8]))
	if n == 0 {
		return "", nil
	}
	raw := make([]uint16, n/2)
	if err := read(buf, unsafe.Slice((*byte)(unsafe.Pointer(&raw[0])), n)); err != nil {
		return "", err
	}
	return windows.UTF16ToString(raw), nil
}

// SplitCommandLine splits as CommandLineToArgvW does, which is how most
// programs parse their own command line.
func SplitCommandLine(line string) []string {
	if line == "" {
		return nil
	}
	p, err := windows.UTF16PtrFromString(line)
	if err != nil {
		return nil
	}
	var n int32
	argv, err := windows.CommandLineToArgv(p, &n)
	if err != nil {
		return nil
	}
	defer windows.LocalFree(windows.Handle(uintptr(unsafe.Pointer(argv))))
	out := make([]string, n)
	for i := range out {
		out[i] = windows.UTF16PtrToString(&argv[i][0])
	}
	return out
}

// Orphans finds nothing: a Terminal's processes sit in a kill-on-close job,
// so Windows ends them when pocketd dies.
func Orphans(uid int) ([]Proc, error) { return nil, nil }

func Alive(pid int) bool {
	h, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, uint32(pid))
	if err != nil {
		return errors.Is(err, windows.ERROR_ACCESS_DENIED)
	}
	defer windows.CloseHandle(h)
	var code uint32
	return windows.GetExitCodeProcess(h, &code) == nil && code == 259 // STILL_ACTIVE
}

// Terminate ends pid at once: Windows has no SIGTERM to send a console program.
func Terminate(pid int) {
	if p, err := os.FindProcess(pid); err == nil {
		p.Kill()
		p.Release()
	}
}

// Kill ends each pid; grace is unused, as Terminate can't be caught.
func Kill(pids []int, grace time.Duration) {
	for _, pid := range pids {
		Terminate(pid)
	}
}
