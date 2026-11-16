package main

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

type fakeSchedules struct {
	autos map[string]proto.Automation
	ran   []string
}

func (f *fakeSchedules) List() []proto.Automation                          { return nil }
func (f *fakeSchedules) Save(a proto.Automation) (proto.Automation, error) { return a, nil }
func (f *fakeSchedules) Delete(string) error                               { return nil }
func (f *fakeSchedules) Get(id string) (proto.Automation, bool)            { a, ok := f.autos[id]; return a, ok }
func (f *fakeSchedules) RunNow(id string) (proto.Run, error) {
	f.ran = append(f.ran, id)
	return proto.Run{ID: "run-" + id}, nil
}

func post(h http.Handler, path, body, sig string) int {
	req := httptest.NewRequest(http.MethodPost, path, strings.NewReader(body))
	if sig != "" {
		req.Header.Set("X-Anywhere-Signature", sig)
	}
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	return w.Code
}

func TestAWebhookWithABadSignatureIsSkipped(t *testing.T) {
	s := &fakeSchedules{autos: map[string]proto.Automation{
		"signed": {ID: "signed", Webhook: true, WebhookSecret: "s3cret"},
		"open":   {ID: "open", Webhook: true},
		"timed":  {ID: "timed"},
	}}
	h := hooks(s)
	mac := hmac.New(sha256.New, []byte("s3cret"))
	mac.Write([]byte(`{"x":1}`))
	good := hex.EncodeToString(mac.Sum(nil))

	for _, c := range []struct {
		path, sig string
		want      int
	}{
		{"/hooks/signed", "", http.StatusUnauthorized},
		{"/hooks/signed", "00" + good[2:], http.StatusUnauthorized},
		{"/hooks/signed", good, http.StatusAccepted},
		{"/hooks/open", "", http.StatusAccepted},
		{"/hooks/timed", "", http.StatusNotFound},
		{"/hooks/missing", "", http.StatusNotFound},
	} {
		if got := post(h, c.path, `{"x":1}`, c.sig); got != c.want {
			t.Errorf("%s sig %q: %d, want %d", c.path, c.sig, got, c.want)
		}
	}
	if strings.Join(s.ran, ",") != "signed,open" {
		t.Fatalf("ran %v", s.ran)
	}
}
