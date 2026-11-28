package main

import "testing"

func TestAPocketdWithNoConsoleGetsAWindowlessOne(t *testing.T) {
	if !needsWindowless(0, false) {
		t.Fatal("a pocketd with no console keeps none, so every git it runs opens a window")
	}
}

func TestAPocketdAloneOnAConsoleWindowSwapsIt(t *testing.T) {
	if !needsWindowless(1, true) {
		t.Fatal("the scheduled task's console window stays open")
	}
}

func TestAConsoleWithNoWindowOrSharedWithAShellStays(t *testing.T) {
	if needsWindowless(1, false) {
		t.Fatal("a console with no window was swapped")
	}
	if needsWindowless(2, true) {
		t.Fatal("the console of the shell that ran pocketd was taken")
	}
}
