package vt

/*
#cgo CFLAGS: -I${SRCDIR}/../../../../third_party/ghostty/zig-out/include
#cgo LDFLAGS: ${SRCDIR}/../../../../third_party/ghostty/zig-out/lib/ghostty-vt.xcframework/macos-arm64_x86_64/libghostty-vt.a
#include <ghostty/vt.h>
#include <stdint.h>

extern void goWritePty(uintptr_t handle, uint8_t* data, size_t len);

static void write_pty_trampoline(GhosttyTerminal t, void* ud, const uint8_t* data, size_t len) {
	goWritePty((uintptr_t)ud, (uint8_t*)data, len);
}

static GhosttyResult vt_new(GhosttyTerminal* out, uint16_t cols, uint16_t rows, uintptr_t handle) {
	GhosttyResult r = ghostty_terminal_new(NULL, out, cols, rows);
	if (r != GHOSTTY_SUCCESS) return r;
	ghostty_terminal_set(*out, GHOSTTY_TERMINAL_OPT_USERDATA, (void*)handle);
	return ghostty_terminal_set(*out, GHOSTTY_TERMINAL_OPT_WRITE_PTY, (const void*)write_pty_trampoline);
}

static GhosttyResult vt_format(GhosttyTerminal t, GhosttyFormatterFormat emit, uint8_t** buf, size_t* len) {
	GhosttyFormatterTerminalOptions o = GHOSTTY_INIT_SIZED(GhosttyFormatterTerminalOptions);
	o.emit = emit;
	o.trim = emit == GHOSTTY_FORMATTER_FORMAT_PLAIN;
	o.extra = (GhosttyFormatterTerminalExtra)GHOSTTY_INIT_SIZED(GhosttyFormatterTerminalExtra);
	o.extra.screen = (GhosttyFormatterScreenExtra)GHOSTTY_INIT_SIZED(GhosttyFormatterScreenExtra);
	if (emit == GHOSTTY_FORMATTER_FORMAT_VT) {
		o.extra.modes = true;
		o.extra.keyboard = true;
		o.extra.screen.cursor = true;
		o.extra.screen.style = true;
		o.extra.screen.kitty_keyboard = true;
	}
	GhosttyFormatter f;
	GhosttyResult r = ghostty_formatter_terminal_new(NULL, &f, t, o);
	if (r != GHOSTTY_SUCCESS) return r;
	r = ghostty_formatter_format_alloc(f, NULL, buf, len);
	ghostty_formatter_free(f);
	return r;
}
*/
import "C"

import (
	"fmt"
	"runtime/cgo"
	"unsafe"
)

// VT is a headless terminal. It is not safe for concurrent use.
type VT struct {
	term   C.GhosttyTerminal
	handle cgo.Handle
}

//export goWritePty
func goWritePty(handle C.uintptr_t, data *C.uint8_t, n C.size_t) {
	reply := cgo.Handle(handle).Value().(func([]byte))
	reply(C.GoBytes(unsafe.Pointer(data), C.int(n)))
}

// New creates a terminal. reply receives the terminal's answers to queries
// such as cursor position reports, which belong on the PTY.
func New(cols, rows int, reply func([]byte)) (*VT, error) {
	v := &VT{handle: cgo.NewHandle(reply)}
	if r := C.vt_new(&v.term, C.uint16_t(cols), C.uint16_t(rows), C.uintptr_t(v.handle)); r != C.GHOSTTY_SUCCESS {
		v.handle.Delete()
		return nil, fmt.Errorf("ghostty_terminal_new: %d", r)
	}
	return v, nil
}

func (v *VT) Write(b []byte) {
	if len(b) > 0 {
		C.ghostty_terminal_vt_write(v.term, (*C.uint8_t)(unsafe.Pointer(&b[0])), C.size_t(len(b)))
	}
}

func (v *VT) Resize(cols, rows int) {
	C.ghostty_terminal_resize(v.term, C.uint16_t(cols), C.uint16_t(rows), 0, 0)
}

func (v *VT) format(emit C.GhosttyFormatterFormat) []byte {
	var buf *C.uint8_t
	var n C.size_t
	if C.vt_format(v.term, emit, &buf, &n) != C.GHOSTTY_SUCCESS {
		return nil
	}
	defer C.ghostty_free(nil, buf, n)
	return C.GoBytes(unsafe.Pointer(buf), C.int(n))
}

// Plain is the visible text with trailing blanks trimmed.
func (v *VT) Plain() string { return string(v.format(C.GHOSTTY_FORMATTER_FORMAT_PLAIN)) }

// Snapshot is a VT byte stream that redraws the current screen, modes and cursor.
func (v *VT) Snapshot() []byte { return v.format(C.GHOSTTY_FORMATTER_FORMAT_VT) }

func (v *VT) Free() {
	C.ghostty_terminal_free(v.term)
	v.handle.Delete()
}
