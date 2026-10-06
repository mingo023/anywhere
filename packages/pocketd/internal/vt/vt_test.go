package vt

import (
	"strings"
	"testing"
)

func newVT(t *testing.T, reply func([]byte)) *VT {
	t.Helper()
	v, err := New(20, 4, reply)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(v.Free)
	return v
}

func TestPlainShowsWrittenText(t *testing.T) {
	v := newVT(t, func([]byte) {})
	v.Write([]byte("hello\r\nworld"))
	if got := v.Plain(); got != "hello\nworld" {
		t.Fatalf("Plain() = %q", got)
	}
}

func TestAnswersCursorPositionQuery(t *testing.T) {
	var reply []byte
	v := newVT(t, func(b []byte) { reply = append(reply, b...) })
	v.Write([]byte("ab\x1b[6n"))
	if string(reply) != "\x1b[1;3R" {
		t.Fatalf("reply = %q", reply)
	}
}

func TestSnapshotRedrawsScreen(t *testing.T) {
	src := newVT(t, func([]byte) {})
	src.Write([]byte("\x1b[1mbold\x1b[0m\r\nline two"))
	dst := newVT(t, func([]byte) {})
	dst.Write(src.Snapshot())
	if got := dst.Plain(); got != src.Plain() {
		t.Fatalf("copy = %q, want %q", got, src.Plain())
	}
	if !strings.Contains(string(src.Snapshot()), "bold") {
		t.Fatal("snapshot lost text")
	}
}

func TestSnapshotKeepsKittyKeyboardFlags(t *testing.T) {
	src := newVT(t, func([]byte) {})
	src.Write([]byte("\x1b[?1049h\x1b[>5u"))
	var got []byte
	dst := newVT(t, func(b []byte) { got = append(got, b...) })
	dst.Write(src.Snapshot())
	dst.Write([]byte("\x1b[?u"))
	if string(got) != "\x1b[?5u" {
		t.Fatalf("flags after snapshot = %q, want %q", got, "\x1b[?5u")
	}
}
