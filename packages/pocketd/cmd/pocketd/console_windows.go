package main

import (
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"syscall"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

// notifyResize does nothing: Windows has no SIGWINCH, so `pocketd run` keeps its first size.
func notifyResize(chan<- os.Signal) {}

var (
	kernel32              = windows.NewLazySystemDLL("kernel32.dll")
	getConsoleProcessList = kernel32.NewProc("GetConsoleProcessList")
	getConsoleWindow      = kernel32.NewProc("GetConsoleWindow")
	freeConsole           = kernel32.NewProc("FreeConsole")
	attachConsole         = kernel32.NewProc("AttachConsole")
)

// detachConsole gives pocketd a console with no window, in place of the
// window Windows opened for a pocketd the scheduled task started, or of no
// console at all.
func detachConsole() {
	var pids [2]uint32
	n, _, _ := getConsoleProcessList.Call(uintptr(unsafe.Pointer(&pids[0])), uintptr(len(pids)))
	window, _, _ := getConsoleWindow.Call()
	if !needsWindowless(int(n), window != 0) {
		return
	}
	freeConsole.Call()
	if !attachWindowless() {
		log.Printf("console: no windowless console to attach, so each console program pocketd runs opens a window")
	}
}

func attachWindowless() bool {
	cmd := exec.Command(filepath.Join(os.Getenv("SystemRoot"), "System32", "cmd.exe"), "/d")
	cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: windows.CREATE_NO_WINDOW}
	stdin, err := cmd.StdinPipe()
	if err != nil || cmd.Start() != nil {
		return false
	}
	// Closing stdin ends cmd.exe, and pocketd keeps the console it attached to.
	defer cmd.Wait()
	defer stdin.Close()
	// cmd.exe's console may not exist yet just after it starts.
	for range 50 {
		if ok, _, _ := attachConsole.Call(uintptr(cmd.Process.Pid)); ok != 0 {
			return true
		}
		time.Sleep(20 * time.Millisecond)
	}
	return false
}
