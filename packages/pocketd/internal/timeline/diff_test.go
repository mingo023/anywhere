package timeline

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestFileDiffSmallEditStaysSmall(t *testing.T) {
	d := FileDiff("a\nb\nc\nd\n", "a\nB\nc\nd\n")
	want := []proto.DiffLine{
		{Number: 1, Kind: "context", Text: "a"},
		{Number: 2, Kind: "del", Text: "b"},
		{Number: 2, Kind: "add", Text: "B"},
		{Number: 3, Kind: "context", Text: "c"},
		{Number: 4, Kind: "context", Text: "d"},
	}
	if !reflect.DeepEqual(d.Lines, want) || d.Additions != 1 || d.Deletions != 1 || d.ContentOnly {
		t.Fatalf("%+v", d)
	}
}

func TestFileDiffNewFileIsContentOnly(t *testing.T) {
	d := FileDiff("", "x\ny")
	if !d.ContentOnly || d.Additions != 2 || len(d.Lines) != 2 {
		t.Fatalf("%+v", d)
	}
}

func TestFileDiffCapsPreview(t *testing.T) {
	long := strings.Repeat("é", 200)
	d := FileDiff("", strings.Repeat(long+"\n", 30))
	if len(d.Lines) != proto.DiffPreviewLines || d.Additions != 30 {
		t.Fatalf("lines=%d additions=%d", len(d.Lines), d.Additions)
	}
	if got := []rune(d.Lines[0].Text); len(got) != proto.DiffLineChars+1 || got[len(got)-1] != '…' {
		t.Fatalf("not capped: %d runes", len(got))
	}
}

func TestParseUnified(t *testing.T) {
	d := ParseUnified("--- a/x\n+++ b/x\n@@ -3,3 +3,3 @@\n keep\n-old\n+new\n")
	want := []proto.DiffLine{
		{Number: 3, Kind: "context", Text: "keep"},
		{Number: 4, Kind: "del", Text: "old"},
		{Number: 4, Kind: "add", Text: "new"},
	}
	if !reflect.DeepEqual(d.Lines, want) {
		t.Fatalf("%+v", d.Lines)
	}
}

func TestDetail(t *testing.T) {
	cases := []struct {
		name, input string
		want        proto.ToolDetail
	}{
		{"Bash", `{"command":"ls","description":"list"}`, proto.ToolDetail{Kind: "shell", Command: "ls", Description: "list"}},
		{"Read", `{"file_path":"/a"}`, proto.ToolDetail{Kind: "read", Path: "/a"}},
		{"Grep", `{"pattern":"x"}`, proto.ToolDetail{Kind: "search", Query: "x"}},
		{"Agent", `{"description":"review"}`, proto.ToolDetail{Kind: "task", Description: "review"}},
		{"Edit", `{"file_path":"/a"}`, proto.ToolDetail{Kind: "edit", Path: "/a"}},
		{"mcp__x", `{"q":1}`, proto.ToolDetail{Kind: "other", Name: "mcp__x", Input: json.RawMessage(`{"q":1}`)}},
	}
	for _, c := range cases {
		if got := Detail(c.name, json.RawMessage(c.input)); !reflect.DeepEqual(got, c.want) {
			t.Errorf("%s: got %+v", c.name, got)
		}
	}
	if d := Detail("Write", json.RawMessage(`{"file_path":"/a","content":"x"}`)); d.Diff == nil || !d.Diff.ContentOnly {
		t.Errorf("Write: %+v", d)
	}
	if d := Detail("Edit", json.RawMessage(`{"file_path":"/a","old_string":"x","new_string":"y"}`)); d.Diff == nil || d.Diff.Deletions != 1 {
		t.Errorf("Edit: %+v", d)
	}
}

func TestParseUnifiedKeepsDashedLinesInsideHunks(t *testing.T) {
	d := ParseUnified("--- a/x.md\n+++ b/x.md\n@@ -1,3 +1,2 @@\n title\n----\n+++ plus\n-body\n")
	want := []proto.DiffLine{
		{Number: 1, Kind: "context", Text: "title"},
		{Number: 2, Kind: "del", Text: "---"},
		{Number: 2, Kind: "add", Text: "++ plus"},
		{Number: 3, Kind: "del", Text: "body"},
	}
	if !reflect.DeepEqual(d.Lines, want) || d.Additions != 1 || d.Deletions != 2 {
		t.Fatalf("%+v", d)
	}
}

func TestParseUnifiedReadsOnlyTheFirstFile(t *testing.T) {
	d := ParseUnified("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n\\ No newline at end of file\ndiff --git a/y b/y\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-c\n+d\n")
	if d.Additions != 1 || d.Deletions != 1 || len(d.Lines) != 2 {
		t.Fatalf("%+v", d)
	}
}
