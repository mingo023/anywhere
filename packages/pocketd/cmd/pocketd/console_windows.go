package main

import (
	"os"
	"unsafe"

	"golang.org/x/sys/windows"
)

// notifyResize does nothing: Windows has no SIGWINCH, so `pocketd run` keeps its first size.
func notifyResize(chan<- os.Signal) {}

var (
	kernel32              = windows.NewLazySystemDLL("kernel32.dll")
	getConsoleProcessList = kernel32.NewProc("GetConsoleProcessList")
	freeConsole           = kernel32.NewProc("FreeConsole")
)

// detachConsole closes the console window Windows opened for a pocketd the
// scheduled task started. A console pocketd shares with its shell stays.
func detachConsole() {
	var pids [2]uint32
	if n, _, _ := getConsoleProcessList.Call(uintptr(unsafe.Pointer(&pids[0])), uintptr(len(pids))); n == 1 {
		freeConsole.Call()
	}
}
