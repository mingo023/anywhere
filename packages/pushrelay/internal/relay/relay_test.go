package relay

import (
	"bytes"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"pushrelay/internal/apns"
	"pushrelay/internal/handle"
)

// fakeAPNs is a real HTTP/2 TLS server answering as APNs does.
type fakeAPNs struct {
	mu     sync.Mutex
	pushes []pushed
	answer func(n int) (int, string)
}

type pushed struct {
	proto  int
	path   string
	header http.Header
	body   string
}

func (f *fakeAPNs) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	body, _ := io.ReadAll(r.Body)
	f.mu.Lock()
	f.pushes = append(f.pushes, pushed{r.ProtoMajor, r.URL.Path, r.Header.Clone(), string(body)})
	n := len(f.pushes)
	f.mu.Unlock()
	status, reason := 200, ""
	if f.answer != nil {
		status, reason = f.answer(n)
	}
	w.Header().Set("apns-id", r.Header.Get("apns-id"))
	w.WriteHeader(status)
	if reason != "" {
		json.NewEncoder(w).Encode(map[string]string{"reason": reason})
	}
}

func (f *fakeAPNs) sent() []pushed {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]pushed(nil), f.pushes...)
}

type harness struct {
	t     *testing.T
	url   string
	now   time.Time
	apns  *fakeAPNs
	relay *Server
}

func newHarness(t *testing.T) *harness {
	f := &fakeAPNs{}
	fake := httptest.NewUnstartedServer(f)
	fake.EnableHTTP2 = true
	fake.StartTLS()
	t.Cleanup(fake.Close)
	key, _ := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	sealer, _ := handle.NewSealer(bytes.Repeat([]byte{7}, 32), bytes.Repeat([]byte{8}, 32))
	h := &harness{t: t, now: time.Unix(1761300000, 0), apns: f}
	now := func() time.Time { return h.now }
	h.relay = &Server{Bundle: "dev.mingo.anywhere", Sealer: sealer, Now: now,
		APNs: &apns.Client{KeyID: "K", TeamID: "T", Key: key, HTTP: fake.Client(), Now: now,
			Hosts: map[string]string{"development": fake.URL, "production": fake.URL}}}
	srv := httptest.NewServer(h.relay.Handler())
	t.Cleanup(srv.Close)
	h.url = srv.URL
	return h
}

type registered struct {
	Handle    string `json:"handle"`
	Secret    string `json:"secret"`
	ExpiresAt int64  `json:"expiresAt"`
}

func (h *harness) register(token, kind, env string) registered {
	h.t.Helper()
	res, err := http.Post(h.url+"/v1/register", "application/json", strings.NewReader(`{"token":"`+token+`","kind":"`+kind+`","env":"`+env+`"}`))
	if err != nil || res.StatusCode != 200 {
		h.t.Fatalf("%v %v", res.Status, err)
	}
	defer res.Body.Close()
	var r registered
	json.NewDecoder(res.Body).Decode(&r)
	return r
}

// send signs body with secret at ts, as pocketd does.
func (h *harness) sendAt(reg registered, ts int64, body string) (int, map[string]any) {
	h.t.Helper()
	secret, _ := base64.RawURLEncoding.DecodeString(reg.Secret)
	stamp := strconv.FormatInt(ts, 10)
	req, _ := http.NewRequest(http.MethodPost, h.url+"/v1/send", strings.NewReader(body))
	req.Header.Set("X-Anywhere-Timestamp", stamp)
	req.Header.Set("X-Anywhere-Signature", handle.Sign(secret, stamp, []byte(body)))
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		h.t.Fatal(err)
	}
	defer res.Body.Close()
	var out map[string]any
	json.NewDecoder(res.Body).Decode(&out)
	return res.StatusCode, out
}

func (h *harness) send(reg registered, body string) (int, map[string]any) {
	return h.sendAt(reg, h.now.Unix(), body)
}

func sendBody(reg registered, priority int, payload string) string {
	return `{"handle":"` + reg.Handle + `","priority":` + strconv.Itoa(priority) + `,"expiration":1761303600,"collapseId":"c1","payload":` + payload + `}`
}

const alertPayload = `{"aps":{"alert":{"title":"Anywhere","body":"A session needs you"},"mutable-content":1,"category":"PERM"},"e":"x"}`
const token = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90"

func TestARegisteredAlertHandleReachesItsTokenThroughAPNs(t *testing.T) {
	h := newHarness(t)
	reg := h.register(token, "alert", "development")
	if reg.ExpiresAt != h.now.Add(HandleTTL).Unix() || reg.Secret == "" || strings.Contains(reg.Handle, token) {
		t.Fatalf("%+v", reg)
	}
	status, body := h.send(reg, sendBody(reg, 10, alertPayload))
	if status != 200 || body["apnsId"] == "" {
		t.Fatalf("%d %v", status, body)
	}
	p := h.apns.sent()
	if len(p) != 1 {
		t.Fatalf("%d pushes", len(p))
	}
	got := p[0]
	if got.proto != 2 || got.path != "/3/device/"+token || got.body != alertPayload {
		t.Fatalf("%+v", got)
	}
	if got.header.Get("apns-topic") != "dev.mingo.anywhere" || got.header.Get("apns-push-type") != "alert" ||
		got.header.Get("apns-priority") != "10" || got.header.Get("apns-collapse-id") != "c1" || got.header.Get("apns-expiration") != "1761303600" {
		t.Fatalf("%v", got.header)
	}
	if body["apnsId"] != got.header.Get("apns-id") {
		t.Fatalf("%v vs %v", body["apnsId"], got.header.Get("apns-id"))
	}
}

func TestActivityAndStartHandlesPushAsLiveActivitiesWithoutCollapse(t *testing.T) {
	h := newHarness(t)
	activity := h.register(token, "activity", "production")
	start := h.register(token, "start", "production")
	if s, b := h.send(activity, sendBody(activity, 5, `{"aps":{"event":"update","content-state":{"working":1}}}`)); s != 200 {
		t.Fatalf("%d %v", s, b)
	}
	if s, b := h.send(start, sendBody(start, 10, `{"aps":{"event":"start","alert":{"title":"a","body":"b"},"content-state":{}}}`)); s != 200 {
		t.Fatalf("%d %v", s, b)
	}
	for _, p := range h.apns.sent() {
		if p.header.Get("apns-topic") != "dev.mingo.anywhere.push-type.liveactivity" || p.header.Get("apns-push-type") != "liveactivity" || p.header.Get("apns-collapse-id") != "" {
			t.Fatalf("%v", p.header)
		}
	}
}

func TestARegistrationThatIsNotATokenKindAndEnvIsRefused(t *testing.T) {
	h := newHarness(t)
	for _, body := range []string{
		`{"token":"zz","kind":"alert","env":"production"}`,
		`{"token":"` + token + `","kind":"background","env":"production"}`,
		`{"token":"` + token + `","kind":"alert","env":"staging"}`,
		`not json`,
	} {
		res, _ := http.Post(h.url+"/v1/register", "application/json", strings.NewReader(body))
		if res.StatusCode != 400 {
			t.Errorf("%s: %d", body, res.StatusCode)
		}
	}
}

func TestASendIsRefusedForABadSignatureStaleTimestampOrExpiredHandle(t *testing.T) {
	h := newHarness(t)
	reg := h.register(token, "alert", "production")
	body := sendBody(reg, 10, alertPayload)
	if s, b := h.sendAt(reg, h.now.Unix()-301, body); s != 401 || b["reason"] != "BadTimestamp" {
		t.Fatalf("stale: %d %v", s, b)
	}
	if s, b := h.sendAt(reg, h.now.Unix()+301, body); s != 401 || b["reason"] != "BadTimestamp" {
		t.Fatalf("future: %d %v", s, b)
	}
	if s, _ := h.sendAt(reg, h.now.Unix()-300, body); s != 200 {
		t.Fatalf("300 s old: %d", s)
	}
	other := h.register(token, "alert", "production")
	forged := registered{Handle: reg.Handle, Secret: other.Secret}
	if s, b := h.send(forged, body); s != 401 || b["reason"] != "BadSignature" {
		t.Fatalf("forged: %d %v", s, b)
	}
	h.now = h.now.Add(HandleTTL)
	if s, b := h.send(reg, body); s != 410 || b["reason"] != "ExpiredHandle" {
		t.Fatalf("expired: %d %v", s, b)
	}
	if n := len(h.apns.sent()); n != 1 {
		t.Fatalf("%d pushes reached APNs", n)
	}
}

func TestAPayloadOverTheCapOrNotFittingItsKindIsRefused(t *testing.T) {
	h := newHarness(t)
	alert := h.register(token, "alert", "production")
	activity := h.register(token, "activity", "production")
	start := h.register(token, "start", "production")
	big := `{"aps":{"alert":"` + strings.Repeat("x", MaxPayload) + `"}}`
	if s, b := h.send(alert, sendBody(alert, 10, big)); s != 413 || b["reason"] != "PayloadTooLarge" {
		t.Fatalf("big: %d %v", s, b)
	}
	for _, c := range []struct {
		reg     registered
		payload string
	}{
		{alert, `{"aps":{"event":"update"}}`},
		{alert, `{"aps":{"content-available":1}}`},
		{alert, `{"aps":{"alert":"x","sound":{"critical":1,"name":"default"}}}`},
		{alert, `{"e":"no aps"}`},
		{activity, `{"aps":{"event":"start","alert":"x"}}`},
		{activity, `{"aps":{"content-state":{}}}`},
		{start, `{"aps":{"event":"start"}}`},
		{start, `{"aps":{"event":"update","alert":"x"}}`},
	} {
		if s, b := h.send(c.reg, sendBody(c.reg, 10, c.payload)); s != 400 || b["reason"] != "BadPayload" {
			t.Errorf("%s: %d %v", c.payload, s, b)
		}
	}
	if s, b := h.send(alert, sendBody(alert, 7, alertPayload)); s != 400 || b["reason"] != "BadPriority" {
		t.Fatalf("priority: %d %v", s, b)
	}
	long := strings.Replace(sendBody(alert, 10, alertPayload), `"c1"`, `"`+strings.Repeat("c", 65)+`"`, 1)
	if s, b := h.send(alert, long); s != 400 || b["reason"] != "BadCollapseId" {
		t.Fatalf("collapse id: %d %v", s, b)
	}
	if n := len(h.apns.sent()); n != 0 {
		t.Fatalf("%d pushes reached APNs", n)
	}
}

func TestAnAlertHandleIsLimitedToItsBurstThenRefillsAtItsHourlyRate(t *testing.T) {
	h := newHarness(t)
	reg := h.register(token, "alert", "production")
	body := sendBody(reg, 10, alertPayload)
	for i := range 10 {
		if s, b := h.send(reg, body); s != 200 {
			t.Fatalf("send %d: %d %v", i, s, b)
		}
	}
	s, b := h.send(reg, body)
	if s != 429 || b["reason"] != "RateLimited" || b["resetsAt"] != h.now.Add(30*time.Second).UTC().Format(time.RFC3339) {
		t.Fatalf("%d %v", s, b)
	}
	other := h.register(token, "alert", "production")
	if s, _ := h.send(other, sendBody(other, 10, alertPayload)); s != 200 {
		t.Fatalf("another handle was limited: %d", s)
	}
	h.now = h.now.Add(30 * time.Second)
	if s, _ := h.send(reg, body); s != 200 {
		t.Fatalf("no refill: %d", s)
	}
}

func TestOneAddressIsLimitedAcrossHandles(t *testing.T) {
	h := newHarness(t)
	for i := range int(RegistersPerIP.Burst) {
		h.register(token[:len(token)-2]+strconv.FormatInt(int64(i%10), 10)+"0", "alert", "production")
	}
	res, _ := http.Post(h.url+"/v1/register", "application/json", strings.NewReader(`{"token":"`+token+`","kind":"alert","env":"production"}`))
	if res.StatusCode != 429 {
		t.Fatalf("%d", res.StatusCode)
	}
}

func TestAPNsDroppingTheTokenPassesThroughAs410AndBusyAs503(t *testing.T) {
	h := newHarness(t)
	reg := h.register(token, "alert", "production")
	body := sendBody(reg, 10, alertPayload)
	for _, c := range []struct {
		status int
		reason string
		want   int
		field  string
		value  any
	}{
		{410, "Unregistered", 410, "reason", "Unregistered"},
		{400, "BadDeviceToken", 410, "reason", "BadDeviceToken"},
		{400, "DeviceTokenNotForTopic", 410, "reason", "DeviceTokenNotForTopic"},
		{429, "TooManyRequests", 503, "retryAfter", 60.0},
		{503, "ServiceUnavailable", 503, "retryAfter", 30.0},
		{400, "BadExpirationDate", 502, "reason", "BadExpirationDate"},
	} {
		h.apns.answer = func(int) (int, string) { return c.status, c.reason }
		h.now = h.now.Add(time.Minute)
		if s, b := h.send(reg, body); s != c.want || b[c.field] != c.value {
			t.Errorf("APNs %d %s: %d %v", c.status, c.reason, s, b)
		}
	}
}
