package factory

import (
	"encoding/base64"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func goBackend() proto.Profile {
	return proto.Profile{ProfileInfo: proto.ProfileInfo{Name: "Go backend", Provider: "claude", Model: "sonnet"}, Soul: "You are careful.", Instructions: "Write the test first.",
		Files: []proto.ProfileFile{
			{Path: "skills/review/SKILL.md", Text: "# Review\n"},
			{Path: "skills/review/logo.png", Text: base64.StdEncoding.EncodeToString([]byte{0xff, 0xd8, 0x00}), Binary: true},
			{Path: "skills/test/SKILL.md", Text: "# Test\n"},
		}}
}

func TestTheListStartsWithTheThreeBuiltInProfiles(t *testing.T) {
	list := OpenProfiles(t.TempDir()).List()
	if len(list) != 3 || list[0].ID != "claude-opus" || list[1].ID != "claude-sonnet" || list[2].ID != "codex" || !list[0].BuiltIn || list[0].Model != "opus" || list[2].Provider != "codex" {
		t.Fatalf("%+v", list)
	}
}

func TestASavedProfileGetsAnIdFromItsNameAndComesBackWhole(t *testing.T) {
	home := t.TempDir()
	saved, err := OpenProfiles(home).Save(goBackend())
	if err != nil || saved.ID != "go-backend" || saved.Skills != 2 {
		t.Fatalf("%+v %v", saved.ProfileInfo, err)
	}
	got, ok := OpenProfiles(home).Get("go-backend")
	want := goBackend()
	want.ID, want.Skills = "go-backend", 2
	if !ok || got.Soul != want.Soul || got.Instructions != want.Instructions || got.ProfileInfo != want.ProfileInfo || len(got.Files) != 3 {
		t.Fatalf("%+v", got)
	}
	for i, f := range got.Files {
		if f != want.Files[i] {
			t.Errorf("file %d: %+v, want %+v", i, f, want.Files[i])
		}
	}
	if _, err := os.Stat(filepath.Join(home, "state", "factory", "profiles", "go-backend", "SOUL.md")); err != nil {
		t.Fatal(err)
	}
	if list := OpenProfiles(home).List(); len(list) != 4 || list[3].Name != "Go backend" {
		t.Fatalf("%+v", list)
	}
}

func TestSavingAProfileAgainReplacesItsSkills(t *testing.T) {
	p := OpenProfiles(t.TempDir())
	saved, _ := p.Save(goBackend())
	saved.Files = saved.Files[2:]
	if again, err := p.Save(saved); err != nil || again.ID != "go-backend" || again.Skills != 1 {
		t.Fatalf("%+v %v", again.ProfileInfo, err)
	}
	if got, _ := p.Get("go-backend"); len(got.Files) != 1 || got.Files[0].Path != "skills/test/SKILL.md" {
		t.Fatalf("%+v", got.Files)
	}
}

func TestTwoProfilesCantShareANameWhateverTheCase(t *testing.T) {
	p := OpenProfiles(t.TempDir())
	p.Save(goBackend())
	twin := goBackend()
	twin.Name = "GO BACKEND"
	if _, err := p.Save(twin); !errors.Is(err, ErrInvalidProfile) || !strings.Contains(err.Error(), "Go backend") {
		t.Fatalf("got %v", err)
	}
	twin.Name = "claude · opus"
	if _, err := p.Save(twin); !errors.Is(err, ErrInvalidProfile) {
		t.Fatalf("a built-in's name: %v", err)
	}
	twin.Name = "Go backend!"
	if saved, err := p.Save(twin); err != nil || saved.ID != "go-backend-2" {
		t.Fatalf("%+v %v", saved.ProfileInfo, err)
	}
}

func TestABuiltInProfileCantBeSavedOrDeleted(t *testing.T) {
	p := OpenProfiles(t.TempDir())
	doc, _ := p.Get("codex")
	doc.Soul = "x"
	if _, err := p.Save(doc); !errors.Is(err, ErrBuiltIn) {
		t.Fatalf("save: %v", err)
	}
	if err := p.Delete("codex", nil); !errors.Is(err, ErrBuiltIn) {
		t.Fatalf("delete: %v", err)
	}
}

func TestAProfileOffTheRulesIsInvalid(t *testing.T) {
	big := strings.Repeat("x", proto.MaxProfileText/2+1)
	for name, change := range map[string]func(*proto.Profile){
		"no name":                 func(p *proto.Profile) { p.Name = "  " },
		"a provider with no path": func(p *proto.Profile) { p.Provider = "cursor" },
		"a model that isn't one":  func(p *proto.Profile) { p.Model = "-x" },
		"an id that is a path":    func(p *proto.Profile) { p.ID = "../x" },
		"soul plus instructions":  func(p *proto.Profile) { p.Soul, p.Instructions = big, big },
		"the whole profile":       func(p *proto.Profile) { p.Files[0].Text = strings.Repeat("x", proto.MaxProfileBytes) },
		"a parent folder":         func(p *proto.Profile) { p.Files[0].Path = "skills/review/../../x/SKILL.md" },
		"an absolute path":        func(p *proto.Profile) { p.Files[0].Path = "/etc/SKILL.md" },
		"a file outside skills":   func(p *proto.Profile) { p.Files[0].Path = "SOUL.md" },
		"a file beside the skill": func(p *proto.Profile) { p.Files[0].Path = "skills/SKILL.md" },
		"a backslash":             func(p *proto.Profile) { p.Files[0].Path = `skills\review\SKILL.md` },
		"a skill with no SKILL":   func(p *proto.Profile) { p.Files = p.Files[1:2] },
		"the same file twice":     func(p *proto.Profile) { p.Files[2] = p.Files[0] },
		"text that isn't base64":  func(p *proto.Profile) { p.Files[1].Text = "not base64!" },
	} {
		doc := goBackend()
		change(&doc)
		if err := ValidProfile(doc); !errors.Is(err, ErrInvalidProfile) {
			t.Errorf("%s: got %v", name, err)
		}
	}
	if err := ValidProfile(goBackend()); err != nil {
		t.Fatalf("a good profile: %v", err)
	}
}

func TestAProfileInUseIsKeptAndTheErrorNamesEachUser(t *testing.T) {
	p := OpenProfiles(t.TempDir())
	p.Save(goBackend())
	err := p.Delete("go-backend", []string{"anywhere Workers role", "ANY-12 t1"})
	if !errors.Is(err, ErrInUse) || err.Error() != "Can't delete. Used by anywhere Workers role, ANY-12 t1. Pick another profile there first" {
		t.Fatalf("got %v", err)
	}
	if _, ok := p.Get("go-backend"); !ok {
		t.Fatal("the profile went")
	}
	changed := p.Changed()
	if err := p.Delete("go-backend", nil); err != nil {
		t.Fatal(err)
	}
	select {
	case <-changed:
	default:
		t.Fatal("a delete didn't tell of the change")
	}
	if err := p.Delete("go-backend", nil); !errors.Is(err, ErrUnknownProfile) {
		t.Fatalf("a second delete: %v", err)
	}
}
