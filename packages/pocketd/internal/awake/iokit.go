package awake

/*
#cgo LDFLAGS: -framework IOKit -framework CoreFoundation
#include <stdlib.h>
#include <IOKit/pwr_mgt/IOPMLib.h>

static IOReturn hold(const char *reason, IOPMAssertionID *id) {
	CFStringRef name = CFStringCreateWithCString(kCFAllocatorDefault, reason, kCFStringEncodingUTF8);
	IOReturn r = IOPMAssertionCreateWithName(kIOPMAssertPreventUserIdleSystemSleep, kIOPMAssertionLevelOn, name, id);
	CFRelease(name);
	return r;
}
*/
import "C"

import (
	"fmt"
	"unsafe"
)

// IOKit holds a PreventUserIdleSystemSleep assertion. The kernel drops it if pocketd dies.
type IOKit struct {
	id   C.IOPMAssertionID
	held bool
}

func (s *IOKit) Hold(reason string) error {
	if s.held {
		return nil
	}
	cs := C.CString(reason)
	defer C.free(unsafe.Pointer(cs))
	if r := C.hold(cs, &s.id); r != 0 {
		return fmt.Errorf("IOPMAssertionCreateWithName: %#x", uint32(r))
	}
	s.held = true
	return nil
}

func (s *IOKit) Release() error {
	if !s.held {
		return nil
	}
	if r := C.IOPMAssertionRelease(s.id); r != 0 {
		return fmt.Errorf("IOPMAssertionRelease: %#x", uint32(r))
	}
	s.held = false
	return nil
}
