// Package relay serves /v1/register and /v1/send (docs/mobile/protocol.md, Relay).
// It keeps no token database: a handle carries its token sealed, and only rate
// counters live in memory. It logs ids, kinds, statuses and sizes, never payloads.
package relay

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"io"
	"log"
	"net"
	"net/http"
	"strconv"
	"time"

	"pushrelay/internal/apns"
	"pushrelay/internal/handle"
	"pushrelay/internal/limit"
)

const (
	// MaxPayload is APNs' cap, which the relay checks before APNs would.
	MaxPayload = 4096
	// Skew is how far a send's timestamp may be from the relay's clock.
	Skew = 300 * time.Second
	// HandleTTL is how long a handle opens; the phone registers again on every hello.
	HandleTTL = 30 * 24 * time.Hour
	maxBody   = 16 << 10
)

// Rates per handle by kind, and per sending address.
var (
	Rates = map[string]limit.Rate{
		handle.Alert:    {PerHour: 120, Burst: 10},
		handle.Activity: {PerHour: 900, Burst: 30},
		handle.Start:    {PerHour: 12, Burst: 3},
	}
	SendsPerIP     = limit.Rate{PerHour: 3600, Burst: 120}
	RegistersPerIP = limit.Rate{PerHour: 120, Burst: 20}
)

type Server struct {
	// Bundle is the app's bundle id; the topic of every push derives from it.
	Bundle string
	Sealer *handle.Sealer
	APNs   *apns.Client
	Now    func() time.Time
	// ClientIP is the header a proxy in front puts the caller's address in
	// (Fly-Client-IP on Fly.io); empty takes the socket's.
	ClientIP string
	Log      *log.Logger

	byKind   map[string]*limit.Limiter
	sendIP   *limit.Limiter
	register *limit.Limiter
	mux      *http.ServeMux
}

func (s *Server) init() {
	if s.mux != nil {
		return
	}
	s.byKind = map[string]*limit.Limiter{}
	for k, r := range Rates {
		s.byKind[k] = limit.New(r)
	}
	s.sendIP, s.register = limit.New(SendsPerIP), limit.New(RegistersPerIP)
	s.mux = http.NewServeMux()
	s.mux.HandleFunc("POST /v1/register", s.handleRegister)
	s.mux.HandleFunc("POST /v1/send", s.handleSend)
	s.mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) { w.Write([]byte("ok\n")) })
}

// Handler is the relay's routes; build the Server's fields first.
func (s *Server) Handler() http.Handler {
	s.init()
	return s.mux
}

func (s *Server) now() time.Time {
	if s.Now != nil {
		return s.Now()
	}
	return time.Now()
}

func (s *Server) logf(format string, args ...any) {
	if s.Log != nil {
		s.Log.Printf(format, args...)
	}
}

func (s *Server) ip(r *http.Request) string {
	if s.ClientIP != "" {
		if v := r.Header.Get(s.ClientIP); v != "" {
			return v
		}
	}
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		return r.RemoteAddr
	}
	return host
}

func reply(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(v)
}

func refuse(w http.ResponseWriter, status int, reason string) {
	reply(w, status, map[string]string{"reason": reason})
}

func (s *Server) limited(w http.ResponseWriter, resets time.Time) {
	reply(w, http.StatusTooManyRequests, map[string]string{"reason": "RateLimited", "resetsAt": resets.UTC().Format(time.RFC3339)})
}

func read(w http.ResponseWriter, r *http.Request) ([]byte, bool) {
	body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, maxBody))
	if err != nil {
		refuse(w, http.StatusRequestEntityTooLarge, "BodyTooLarge")
		return nil, false
	}
	return body, true
}

func (s *Server) handleRegister(w http.ResponseWriter, r *http.Request) {
	now := s.now()
	if ok, resets := s.register.Take(s.ip(r), now); !ok {
		s.limited(w, resets)
		return
	}
	body, ok := read(w, r)
	if !ok {
		return
	}
	var req struct{ Token, Kind, Env string }
	if json.Unmarshal(body, &req) != nil || !validToken(req.Token) || Rates[req.Kind] == (limit.Rate{}) ||
		req.Env != handle.Development && req.Env != handle.Production {
		refuse(w, http.StatusBadRequest, "BadRegistration")
		return
	}
	exp := now.Add(HandleTTL).Unix()
	h, err := s.Sealer.Seal(handle.Token{Token: req.Token, Kind: req.Kind, Env: req.Env, Exp: exp})
	if err != nil {
		refuse(w, http.StatusInternalServerError, "SealFailed")
		return
	}
	s.logf("register kind=%s env=%s", req.Kind, req.Env)
	reply(w, http.StatusOK, map[string]any{"handle": h, "secret": s.Sealer.SecretString(h), "expiresAt": exp})
}

// validToken is an APNs device token as hex: 32 bytes today, longer allowed.
func validToken(t string) bool {
	b, err := hex.DecodeString(t)
	return err == nil && len(b) >= 8 && len(b) <= 100
}

type send struct {
	Handle     string          `json:"handle"`
	Priority   int             `json:"priority"`
	Expiration int64           `json:"expiration"`
	CollapseID string          `json:"collapseId"`
	Payload    json.RawMessage `json:"payload"`
}

func (s *Server) handleSend(w http.ResponseWriter, r *http.Request) {
	now := s.now()
	ts := r.Header.Get("X-Anywhere-Timestamp")
	at, err := strconv.ParseInt(ts, 10, 64)
	if err != nil || now.Sub(time.Unix(at, 0)).Abs() > Skew {
		refuse(w, http.StatusUnauthorized, "BadTimestamp")
		return
	}
	body, ok := read(w, r)
	if !ok {
		return
	}
	var req send
	if json.Unmarshal(body, &req) != nil || req.Handle == "" {
		refuse(w, http.StatusBadRequest, "BadRequest")
		return
	}
	if !handle.Verify(s.Sealer.Secret(req.Handle), ts, body, r.Header.Get("X-Anywhere-Signature")) {
		refuse(w, http.StatusUnauthorized, "BadSignature")
		return
	}
	tok, err := s.Sealer.Open(req.Handle)
	if err != nil {
		refuse(w, http.StatusGone, "BadHandle")
		return
	}
	if tok.Exp <= now.Unix() {
		refuse(w, http.StatusGone, "ExpiredHandle")
		return
	}
	var payload bytes.Buffer
	if json.Compact(&payload, req.Payload) != nil {
		refuse(w, http.StatusBadRequest, "BadPayload")
		return
	}
	if payload.Len() > MaxPayload {
		refuse(w, http.StatusRequestEntityTooLarge, "PayloadTooLarge")
		return
	}
	if reason := fits(tok.Kind, payload.Bytes()); reason != "" {
		refuse(w, http.StatusBadRequest, reason)
		return
	}
	if req.Priority != 5 && req.Priority != 10 {
		refuse(w, http.StatusBadRequest, "BadPriority")
		return
	}
	if len(req.CollapseID) > 64 {
		refuse(w, http.StatusBadRequest, "BadCollapseId")
		return
	}
	if ok, resets := s.byKind[tok.Kind].Take(req.Handle, now); !ok {
		s.limited(w, resets)
		return
	}
	if ok, resets := s.sendIP.Take(s.ip(r), now); !ok {
		s.limited(w, resets)
		return
	}
	push := apns.Request{Token: tok.Token, Env: tok.Env, Topic: s.Bundle, PushType: "alert", Priority: req.Priority,
		Expiration: req.Expiration, Payload: payload.Bytes()}
	if tok.Kind == handle.Alert {
		push.CollapseID = req.CollapseID
	} else {
		push.Topic, push.PushType = s.Bundle+".push-type.liveactivity", "liveactivity"
	}
	ctx, cancel := context.WithTimeout(r.Context(), 20*time.Second)
	defer cancel()
	res, err := s.APNs.Send(ctx, push)
	s.logf("send kind=%s status=%d size=%d apnsId=%s", tok.Kind, res.Status, payload.Len(), res.ID)
	switch {
	case err != nil:
		reply(w, http.StatusServiceUnavailable, map[string]int{"retryAfter": 5})
	case res.Status == http.StatusOK:
		reply(w, http.StatusOK, map[string]string{"apnsId": res.ID})
	case res.Status == http.StatusGone,
		res.Status == http.StatusBadRequest && (res.Reason == "BadDeviceToken" || res.Reason == "DeviceTokenNotForTopic"):
		refuse(w, http.StatusGone, res.Reason)
	case res.Status == http.StatusTooManyRequests, res.Status >= 500,
		res.Status == http.StatusForbidden && (res.Reason == "ExpiredProviderToken" || res.Reason == "InvalidProviderToken"):
		reply(w, http.StatusServiceUnavailable, map[string]int{"retryAfter": retryAfter(res)})
	default:
		refuse(w, http.StatusBadGateway, res.Reason)
	}
}

func retryAfter(res apns.Response) int {
	switch {
	case res.RetryAfter > 0:
		return res.RetryAfter
	case res.Status == http.StatusTooManyRequests:
		return 60
	case res.Status == http.StatusForbidden:
		return 1
	}
	return 30
}

// fits says why a payload doesn't suit its handle's kind, or "" when it does:
// only activity handles carry aps.event update or end, only start handles
// event start with an alert; no kind sends background or critical pushes.
func fits(kind string, payload []byte) string {
	var p struct {
		Aps map[string]json.RawMessage `json:"aps"`
	}
	if json.Unmarshal(payload, &p) != nil || p.Aps == nil {
		return "BadPayload"
	}
	if _, ok := p.Aps["content-available"]; ok {
		return "BadPayload"
	}
	var sound struct{ Critical int }
	if raw, ok := p.Aps["sound"]; ok && json.Unmarshal(raw, &sound) == nil && sound.Critical != 0 {
		return "BadPayload"
	}
	var event string
	_, hasEvent := p.Aps["event"]
	if hasEvent && json.Unmarshal(p.Aps["event"], &event) != nil {
		return "BadPayload"
	}
	_, alert := p.Aps["alert"]
	switch kind {
	case handle.Alert:
		if hasEvent {
			return "BadPayload"
		}
	case handle.Activity:
		if event != "update" && event != "end" {
			return "BadPayload"
		}
	case handle.Start:
		if event != "start" || !alert {
			return "BadPayload"
		}
	default:
		return "BadPayload"
	}
	return ""
}
