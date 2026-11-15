package proto

import (
	"encoding/json"
	"reflect"
	"testing"
)

func TestTheProfileGoldensDecodeToTheirFields(t *testing.T) {
	m, err := DecodeClient(factoryGolden(t, "client", "profile_save.json"))
	if err != nil || m.ProfileDoc == nil {
		t.Fatalf("%+v %v", m, err)
	}
	want := Profile{ProfileInfo: ProfileInfo{Name: "Go backend", Provider: "claude", Model: "sonnet"}, Soul: "You are a careful backend engineer.",
		Instructions: "Write a failing test first.", Files: []ProfileFile{{Path: "skills/review/SKILL.md", Text: "# Review\n"}}}
	if !reflect.DeepEqual(*m.ProfileDoc, want) {
		t.Fatalf("got %+v", *m.ProfileDoc)
	}
	for name, id := range map[string]string{"profile_get.json": "go-backend", "profile_delete.json": "go-backend"} {
		if m, err = DecodeClient(factoryGolden(t, "client", name)); err != nil || m.Profile != id {
			t.Fatalf("%s: %+v %v", name, m, err)
		}
	}
	if m, err = DecodeClient(factoryGolden(t, "client", "profile_list.json")); err != nil || m.Type != "factory.profile.list" || m.ID != "factory-1" {
		t.Fatalf("%+v %v", m, err)
	}
	if m, err = DecodeClient(factoryGolden(t, "client", "advance.json")); err != nil || m.TicketID != "t1" {
		t.Fatalf("%+v %v", m, err)
	}
}

func TestAProfileFrameOffTheContractIsMalformed(t *testing.T) {
	for _, raw := range []string{
		`{"type":"factory.profile.save","id":"1","profile":"go-backend"}`,
		`{"type":"factory.profile.save","id":"1","profile":{"provider":"claude"}}`,
		`{"type":"factory.profile.save","id":"1","profile":{"name":"A","provider":"claude","extra":1}}`,
		`{"type":"factory.profile.save","id":"1","profile":{"name":"A","provider":"claude","files":[{"path":"skills/a/SKILL.md","text":"x","mode":7}]}}`,
		`{"type":"factory.profile.save","id":"1","profile":{"name":"A","provider":"claude","files":[{"path":"skills/a/SKILL.md"}]}}`,
		`{"type":"factory.profile.get","id":"1"}`,
		`{"type":"factory.profile.delete","id":"1","profile":""}`,
		`{"type":"factory.advance","id":"1"}`,
		`{"type":"factory.advance","id":"1","ticketId":""}`,
	} {
		if m, err := DecodeClient([]byte(raw)); err == nil {
			t.Errorf("%s decoded to %+v", raw, m)
		}
	}
}

func TestTheProfileGoldensAreWhatTheServerWrites(t *testing.T) {
	doc := Profile{ProfileInfo: ProfileInfo{ID: "go-backend", Name: "Go backend", Provider: "claude", Model: "sonnet", Skills: 1}, Soul: "You are a careful backend engineer.",
		Instructions: "Write a failing test first.", Files: []ProfileFile{{Path: "skills/review/SKILL.md", Text: "# Review\n"}}}
	list := []ProfileInfo{{ID: "claude-opus", Name: "Claude · Opus", Provider: "claude", Model: "opus", BuiltIn: true}, doc.ProfileInfo}
	for name, frame := range map[string]any{"profiles.json": NewFactoryProfiles("factory-1", list), "profile.json": NewFactoryProfile("factory-2", doc)} {
		wrote, _ := json.Marshal(frame)
		var got, want any
		json.Unmarshal(wrote, &got)
		json.Unmarshal(factoryGolden(t, "server", name), &want)
		if !reflect.DeepEqual(got, want) {
			t.Errorf("%s: the server writes %s", name, wrote)
		}
	}
}

func TestATeamStartPickMayNameAProfile(t *testing.T) {
	m, err := DecodeClient([]byte(`{"type":"team.start","id":"1","lead":"a1","tasks":[{"id":"t1","agent":"claude","profile":"go-backend"},{"id":"t2","agent":"codex"}]}`))
	if err != nil || m.Tasks[0].Profile != "go-backend" || m.Tasks[1].Profile != "" {
		t.Fatalf("%+v %v", m, err)
	}
	if _, err := DecodeClient([]byte(`{"type":"team.start","id":"1","lead":"a1","tasks":[{"id":"t1","agent":"claude","profile":7}]}`)); err == nil {
		t.Fatal("a profile that isn't a string decoded")
	}
}
