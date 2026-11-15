package factory

import (
	"cmp"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"sync"
	"unicode"
	"unicode/utf8"

	"pocketd/internal/launch"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
)

var (
	ErrInvalidProfile = errors.New("invalid profile")
	ErrBuiltIn        = errors.New("A built-in profile can't be changed")
	ErrInUse          = errors.New("profile in use")
	ErrUnknownProfile = errors.New("No such profile")
)

// invalid is why a profile can't be saved, as a sentence for the user.
type invalid string

func (e invalid) Error() string      { return string(e) }
func (invalid) Is(target error) bool { return target == ErrInvalidProfile }

// inUse is who would lose a profile if it went.
type inUse []string

func (e inUse) Error() string {
	return "Can't delete. Used by " + strings.Join(e, ", ") + ". Pick another profile there first"
}
func (inUse) Is(target error) bool { return target == ErrInUse }

var (
	profileID = regexp.MustCompile(`^[a-z0-9][a-z0-9-]{0,63}$`)
	skillName = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`)
)

const maxProfileName = 80

// Profiles keeps each saved profile as a folder: profile.json, SOUL.md,
// INSTRUCTIONS.md and skills/<name>/….
type Profiles struct {
	dir     string
	mu      sync.Mutex
	changed chan struct{}
}

func OpenProfiles(home string) *Profiles {
	return &Profiles{dir: filepath.Join(home, "state", "factory", "profiles"), changed: make(chan struct{})}
}

// BuiltIns are the profiles every pocketd has. They exist only here.
func BuiltIns() []proto.Profile {
	return []proto.Profile{
		{ProfileInfo: proto.ProfileInfo{ID: "claude-opus", Name: "Claude · Opus", Provider: providers.Claude.ID, Model: "opus", BuiltIn: true}},
		{ProfileInfo: proto.ProfileInfo{ID: "claude-sonnet", Name: "Claude · Sonnet", Provider: providers.Claude.ID, Model: "sonnet", BuiltIn: true}},
		{ProfileInfo: proto.ProfileInfo{ID: "codex", Name: "Codex", Provider: providers.Codex.ID, BuiltIn: true}},
	}
}

func builtIn(id string) (proto.Profile, bool) {
	all := BuiltIns()
	i := slices.IndexFunc(all, func(p proto.Profile) bool { return p.ID == id })
	if i < 0 {
		return proto.Profile{}, false
	}
	return all[i], true
}

// Changed is closed when a profile is next saved or deleted.
func (p *Profiles) Changed() <-chan struct{} {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.changed
}

func (p *Profiles) bump() {
	close(p.changed)
	p.changed = make(chan struct{})
}

// List is the built-in profiles, then the saved ones by name.
func (p *Profiles) List() []proto.ProfileInfo {
	p.mu.Lock()
	defer p.mu.Unlock()
	out := []proto.ProfileInfo{}
	for _, b := range BuiltIns() {
		out = append(out, b.ProfileInfo)
	}
	for _, s := range p.saved() {
		out = append(out, s.ProfileInfo)
	}
	return out
}

func (p *Profiles) Get(id string) (proto.Profile, bool) {
	if b, ok := builtIn(id); ok {
		return b, true
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.read(id)
}

// Save writes doc. One without an id is new and gets its id from its name.
func (p *Profiles) Save(doc proto.Profile) (proto.Profile, error) {
	doc.Name = strings.TrimSpace(doc.Name)
	if _, ok := builtIn(doc.ID); ok {
		return doc, ErrBuiltIn
	}
	if err := ValidProfile(doc); err != nil {
		return doc, err
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	saved := p.saved()
	for _, other := range append(BuiltIns(), saved...) {
		if other.ID != doc.ID && strings.EqualFold(other.Name, doc.Name) {
			return doc, invalid("Another profile is named " + other.Name)
		}
	}
	known := func(id string) bool {
		_, built := builtIn(id)
		return built || slices.ContainsFunc(saved, func(s proto.Profile) bool { return s.ID == id })
	}
	switch {
	case doc.ID == "":
		doc.ID = freshID(doc.Name, known)
	case !known(doc.ID):
		return doc, ErrUnknownProfile
	}
	doc.BuiltIn, doc.Skills = false, len(skillsOf(doc.Files))
	if err := p.write(doc); err != nil {
		return doc, err
	}
	p.bump()
	return doc, nil
}

// Delete removes a saved profile that users, the list of who runs with it, leaves free.
func (p *Profiles) Delete(id string, users []string) error {
	if _, ok := builtIn(id); ok {
		return ErrBuiltIn
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if _, ok := p.read(id); !ok {
		return ErrUnknownProfile
	}
	if len(users) > 0 {
		return inUse(users)
	}
	if err := os.RemoveAll(filepath.Join(p.dir, id)); err != nil {
		return err
	}
	p.bump()
	return nil
}

// ValidProfile is nil for a profile pocketd can store and launch with.
func ValidProfile(doc proto.Profile) error {
	name := strings.TrimSpace(doc.Name)
	switch {
	case doc.ID != "" && !profileID.MatchString(doc.ID):
		return invalid("That profile id can't be used")
	case name == "" || utf8.RuneCountInString(name) > maxProfileName || strings.ContainsFunc(name, unicode.IsControl):
		return invalid("A profile needs a name of at most " + strconv.Itoa(maxProfileName) + " characters")
	case doc.Provider != providers.Claude.ID && doc.Provider != providers.Codex.ID:
		return invalid("A profile runs Claude or Codex")
	case doc.Model != "" && launch.ValidSwitch(doc.Provider, doc.Model, "") != nil:
		return invalid("That model name can't be used")
	case len(doc.Soul)+len(doc.Instructions) > proto.MaxProfileText:
		return invalid("Soul and instructions are over 64 KB together")
	}
	size, seen := len(doc.Soul)+len(doc.Instructions), map[string]bool{}
	for _, f := range doc.Files {
		parts := strings.Split(f.Path, "/")
		if strings.Contains(f.Path, `\`) || path.IsAbs(f.Path) || path.Clean(f.Path) != f.Path || slices.Contains(parts, "..") ||
			len(parts) < 3 || parts[0] != "skills" || !skillName.MatchString(parts[1]) {
			return invalid("A skill's file must sit in skills/<name>/: " + f.Path)
		}
		if seen[f.Path] {
			return invalid("A file is listed twice: " + f.Path)
		}
		seen[f.Path] = true
		data, err := body(f)
		if err != nil {
			return invalid("A binary file isn't base64: " + f.Path)
		}
		if size += len(data); size > proto.MaxProfileBytes {
			return invalid("The profile is over 2 MB")
		}
	}
	for _, skill := range skillsOf(doc.Files) {
		if !seen["skills/"+skill+"/SKILL.md"] {
			return invalid("Skill " + skill + " has no SKILL.md")
		}
	}
	return nil
}

// body is f's bytes on disk.
func body(f proto.ProfileFile) ([]byte, error) {
	if f.Binary {
		return base64.StdEncoding.DecodeString(f.Text)
	}
	return []byte(f.Text), nil
}

// skillsOf are the names of the skills files hold, sorted.
func skillsOf(files []proto.ProfileFile) []string {
	var names []string
	for _, f := range files {
		if parts := strings.Split(f.Path, "/"); len(parts) >= 3 && !slices.Contains(names, parts[1]) {
			names = append(names, parts[1])
		}
	}
	slices.Sort(names)
	return names
}

// freshID is name as an id no profile has: its letters and digits in lower case, joined by hyphens.
func freshID(name string, taken func(string) bool) string {
	var b strings.Builder
	for _, r := range strings.ToLower(name) {
		switch {
		case r >= 'a' && r <= 'z' || r >= '0' && r <= '9':
			b.WriteRune(r)
		case b.Len() > 0 && !strings.HasSuffix(b.String(), "-"):
			b.WriteByte('-')
		}
	}
	id := strings.Trim(b.String(), "-")
	if id = id[:min(len(id), 40)]; id == "" {
		id = "profile"
	}
	if !taken(id) {
		return id
	}
	for n := 2; ; n++ {
		if next := id + "-" + strconv.Itoa(n); !taken(next) {
			return next
		}
	}
}

type meta struct {
	Name     string `json:"name"`
	Provider string `json:"provider"`
	Model    string `json:"model,omitempty"`
}

// saved reads every stored profile, by name. Callers hold mu.
func (p *Profiles) saved() []proto.Profile {
	entries, _ := os.ReadDir(p.dir)
	var out []proto.Profile
	for _, e := range entries {
		if doc, ok := p.read(e.Name()); ok {
			out = append(out, doc)
		}
	}
	slices.SortFunc(out, func(a, b proto.Profile) int { return cmp.Compare(strings.ToLower(a.Name), strings.ToLower(b.Name)) })
	return out
}

func (p *Profiles) read(id string) (proto.Profile, bool) {
	var doc proto.Profile
	if !profileID.MatchString(id) {
		return doc, false
	}
	dir := filepath.Join(p.dir, id)
	var m meta
	b, err := os.ReadFile(filepath.Join(dir, "profile.json"))
	if err != nil || json.Unmarshal(b, &m) != nil {
		return doc, false
	}
	soul, _ := os.ReadFile(filepath.Join(dir, "SOUL.md"))
	instructions, _ := os.ReadFile(filepath.Join(dir, "INSTRUCTIONS.md"))
	doc.ProfileInfo = proto.ProfileInfo{ID: id, Name: m.Name, Provider: m.Provider, Model: m.Model}
	doc.Soul, doc.Instructions = string(soul), string(instructions)
	filepath.WalkDir(filepath.Join(dir, "skills"), func(file string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		data, err := os.ReadFile(file)
		if err != nil {
			return nil
		}
		rel, _ := filepath.Rel(dir, file)
		f := proto.ProfileFile{Path: filepath.ToSlash(rel), Text: string(data)}
		if !utf8.Valid(data) {
			f.Text, f.Binary = base64.StdEncoding.EncodeToString(data), true
		}
		doc.Files = append(doc.Files, f)
		return nil
	})
	doc.Skills = len(skillsOf(doc.Files))
	return doc, true
}

// write puts doc's folder in place whole: a reader sees the old profile or the new one.
func (p *Profiles) write(doc proto.Profile) error {
	final, fresh, old := filepath.Join(p.dir, doc.ID), filepath.Join(p.dir, "."+doc.ID+".new"), filepath.Join(p.dir, "."+doc.ID+".old")
	os.RemoveAll(fresh)
	os.RemoveAll(old)
	put := func(rel string, data []byte) error {
		file := filepath.Join(fresh, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(file), 0o700); err != nil {
			return err
		}
		return os.WriteFile(file, data, 0o600)
	}
	m, _ := json.MarshalIndent(meta{doc.Name, doc.Provider, doc.Model}, "", "\t")
	for rel, data := range map[string][]byte{"profile.json": m, "SOUL.md": []byte(doc.Soul), "INSTRUCTIONS.md": []byte(doc.Instructions)} {
		if err := put(rel, data); err != nil {
			return err
		}
	}
	for _, f := range doc.Files {
		data, err := body(f)
		if err != nil {
			return err
		}
		if err := put(f.Path, data); err != nil {
			return err
		}
	}
	if err := os.Rename(final, old); err != nil && !errors.Is(err, fs.ErrNotExist) {
		return err
	}
	if err := os.Rename(fresh, final); err != nil {
		return err
	}
	return os.RemoveAll(old)
}
