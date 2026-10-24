package fsview

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/proto"
)

var png = []byte("\x89PNG\r\n\x1a\n rest of the image")

func TestAnUploadLandsOwnerOnlyUnderAttachmentsAsItsIdAndType(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "attachments")
	u, err := StartUpload(dir, "u1", "Screenshot 2026-10-24.PNG")
	if err != nil {
		t.Fatal(err)
	}
	if err := u.Put(0, png[:10]); err != nil {
		t.Fatal(err)
	}
	if err := u.Put(1, png[10:]); err != nil {
		t.Fatal(err)
	}
	path, err := u.Finish()
	if err != nil || path != filepath.Join(dir, "u1.png") {
		t.Fatal(path, err)
	}
	got, _ := os.ReadFile(path)
	fi, _ := os.Stat(path)
	di, _ := os.Stat(dir)
	if !bytes.Equal(got, png) || fi.Mode().Perm() != 0o600 || di.Mode().Perm() != 0o700 {
		t.Fatalf("%q %v %v", got, fi.Mode(), di.Mode())
	}
	if parts, _ := filepath.Glob(filepath.Join(dir, "*.part")); len(parts) != 0 {
		t.Fatal(parts)
	}
}

func TestOnlyTheImageTypesAChatTakesMayBeAttached(t *testing.T) {
	dir := t.TempDir()
	for _, name := range []string{"notes.txt", "icon.svg", "photo.bmp", "noext"} {
		if _, err := StartUpload(dir, "u", name); codeOf(err) != proto.CodeAttachInvalid {
			t.Errorf("%s: %v", name, err)
		}
	}
	for name, head := range map[string][]byte{
		"a.jpg": {0xff, 0xd8, 0xff, 0xe0}, "a.jpeg": {0xff, 0xd8, 0xff, 0xdb}, "a.gif": []byte("GIF89a.."),
		"a.webp": []byte("RIFF\x00\x00\x00\x00WEBPVP8 "), "a.heic": []byte("\x00\x00\x00\x18ftypheic\x00\x00"), "a.png": png,
	} {
		u, err := StartUpload(dir, "ok", name)
		if err != nil {
			t.Fatal(name, err)
		}
		if err := u.Put(0, head); err != nil {
			t.Errorf("%s: %v", name, err)
		}
		u.Abort()
	}
}

func TestAFileThatIsntTheImageItsNameSaysIsRefused(t *testing.T) {
	u, _ := StartUpload(t.TempDir(), "u", "a.png")
	if err := u.Put(0, []byte("#!/bin/sh\nrm -rf ~\n")); codeOf(err) != proto.CodeAttachInvalid {
		t.Fatal(err)
	}
}

func TestChunksMustComeInOrder(t *testing.T) {
	u, _ := StartUpload(t.TempDir(), "u", "a.png")
	if err := u.Put(1, png); codeOf(err) != proto.CodeAttachInvalid {
		t.Fatal(err)
	}
	u.Put(0, png)
	if err := u.Put(0, png); codeOf(err) != proto.CodeAttachInvalid {
		t.Fatal(err)
	}
}

func TestAnUploadOver20MBIsRefused(t *testing.T) {
	u, _ := StartUpload(t.TempDir(), "u", "a.png")
	chunk := append(append([]byte{}, png...), make([]byte, proto.AttachChunk-len(png))...)
	if err := u.Put(0, chunk); err != nil {
		t.Fatal(err)
	}
	var err error
	for i := 1; err == nil; i++ {
		err = u.Put(i, make([]byte, proto.AttachChunk))
		if i > AttachMax/proto.AttachChunk {
			break
		}
	}
	if codeOf(err) != proto.CodeAttachInvalid || u.size > AttachMax {
		t.Fatal(err, u.size)
	}
}

func TestAnAbortedUploadLeavesNothing(t *testing.T) {
	dir := t.TempDir()
	u, _ := StartUpload(dir, "u", "a.png")
	u.Put(0, png)
	u.Abort()
	if left, _ := os.ReadDir(dir); len(left) != 0 {
		t.Fatal(left)
	}
}

func TestTwoUploadsOfOneIdDontWriteIntoEachOther(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "attachments")
	a, _ := StartUpload(dir, "same", "a.png")
	b, _ := StartUpload(dir, "same", "b.png")
	other := []byte("\x89PNG\r\n\x1a\n another image entirely")
	if err := a.Put(0, png); err != nil {
		t.Fatal(err)
	}
	if err := b.Put(0, other); err != nil {
		t.Fatal(err)
	}
	b.Abort()
	path, err := a.Finish()
	if got, _ := os.ReadFile(path); err != nil || !bytes.Equal(got, png) {
		t.Fatalf("%q %v", got, err)
	}
}
