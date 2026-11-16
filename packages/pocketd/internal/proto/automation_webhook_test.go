package proto

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestAWebhookScheduleIsValidOnlyForAWebhookAutomationAndOnlyAlone(t *testing.T) {
	for _, c := range []struct {
		name     string
		schedule Schedule
		webhook  bool
		want     bool
	}{
		{"webhook alone on a webhook automation", Schedule{Kind: "webhook"}, true, true},
		{"webhook on an automation with no webhook", Schedule{Kind: "webhook"}, false, false},
		{"webhook with days", Schedule{Kind: "webhook", Days: []int{1}}, true, false},
		{"webhook with a time", Schedule{Kind: "webhook", Time: "09:00"}, true, false},
		{"webhook with an interval", Schedule{Kind: "webhook", EveryMin: 60}, true, false},
		{"no kind on a webhook automation", Schedule{}, true, false},
		{"days beside a webhook", Schedule{Kind: "days", Days: []int{1}, Time: "09:00"}, true, true},
		{"interval with no webhook", Schedule{Kind: "interval", EveryMin: 60}, false, true},
		{"an unknown kind", Schedule{Kind: "weekly"}, true, false},
	} {
		if got := c.schedule.ValidFor(c.webhook); got != c.want {
			t.Errorf("%s: %v, want %v", c.name, got, c.want)
		}
	}
}

func TestAnAutomationOnAWebhookNeedsTheWebhookScheduleNotNoSchedule(t *testing.T) {
	a := Automation{Name: "n", Prompt: "p", Provider: "claude", Folder: "/f", Enabled: true, Webhook: true}
	if a.Valid() {
		t.Error("a webhook automation with no schedule is valid")
	}
	a.Schedule = Schedule{Kind: "webhook"}
	if !a.Valid() {
		t.Error("a webhook automation with the webhook schedule is invalid")
	}
	a.Webhook = false
	if a.Valid() {
		t.Error("the webhook schedule on an automation with no webhook is valid")
	}
}

func TestAWebhookSecretNeverReachesTheWire(t *testing.T) {
	raw, err := json.Marshal(NewAutomations([]Automation{{ID: "au", Webhook: true, WebhookURL: "http://127.0.0.1:1/hooks/au", WebhookSecret: "s3cret"}}, nil))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), "s3cret") || strings.Contains(string(raw), "webhookSecret") {
		t.Fatalf("the snapshot carries the secret: %s", raw)
	}
	if !strings.Contains(string(raw), `"webhookUrl":"http://127.0.0.1:1/hooks/au"`) {
		t.Fatalf("the snapshot lacks the webhook URL: %s", raw)
	}
}

func TestAClientCanSaveTheWebhookScheduleOfAnAutomationItListedButNotAnythingBeside(t *testing.T) {
	m, err := DecodeClient(golden(t, "client", "automation_save_webhook.json"))
	if err != nil || m.Automation == nil || m.Automation.Schedule.Kind != "webhook" || m.Automation.Webhook {
		t.Fatalf("%+v %v", m, err)
	}
	const save = `{"type":"automation.save","id":"1","automation":{"name":"n","prompt":"p","provider":"claude","folder":"/f","enabled":true,"schedule":`
	for _, schedule := range []string{`{"kind":"webhook","days":[1]}`, `{"kind":"webhook","everyMin":60}`, `{"kind":"webhook","time":"09:00"}`} {
		if _, err := DecodeClient([]byte(save + schedule + `}}`)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", schedule, err)
		}
	}
}
