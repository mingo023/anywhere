package push

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"time"

	"pocketd/internal/proto"
)

var (
	// ErrGone is the relay saying the handle's token is dead: drop it, never retry.
	ErrGone = errors.New("push: handle gone")
	// ErrLimited is the relay's rate limit; the send is dropped.
	ErrLimited = errors.New("push: rate limited")
)

// RetryError is the relay or APNs being busy; try again after After.
type RetryError struct{ After time.Duration }

func (e RetryError) Error() string { return fmt.Sprintf("push: relay busy, retry after %s", e.After) }

// Client sends through the relay at URL, signing each send with its handle's secret.
type Client struct {
	URL  func() string
	HTTP *http.Client
	Now  func() time.Time
}

// Send is the body of /v1/send.
type Send struct {
	Handle     string          `json:"handle"`
	Priority   int             `json:"priority"`
	Expiration int64           `json:"expiration,omitempty"`
	CollapseID string          `json:"collapseId,omitempty"`
	Payload    json.RawMessage `json:"payload"`
}

// Sign is X-Anywhere-Signature: b64url(HMAC-SHA256(secret, ts + "." + body)), secret being the handle's b64url secret.
func Sign(secret, ts string, body []byte) (string, error) {
	key, err := base64.RawURLEncoding.DecodeString(secret)
	if err != nil {
		return "", err
	}
	m := hmac.New(sha256.New, key)
	m.Write([]byte(ts + "."))
	m.Write(body)
	return base64.RawURLEncoding.EncodeToString(m.Sum(nil)), nil
}

func (c *Client) Send(ctx context.Context, h proto.PushHandle, s Send) (apnsID string, err error) {
	s.Handle = h.Handle
	body, err := json.Marshal(s)
	if err != nil {
		return "", err
	}
	now := time.Now
	if c.Now != nil {
		now = c.Now
	}
	ts := strconv.FormatInt(now().Unix(), 10)
	sig, err := Sign(h.Secret, ts, body)
	if err != nil {
		return "", err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.URL()+"/v1/send", bytes.NewReader(body))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Anywhere-Timestamp", ts)
	req.Header.Set("X-Anywhere-Signature", sig)
	hc := c.HTTP
	if hc == nil {
		hc = http.DefaultClient
	}
	res, err := hc.Do(req)
	if err != nil {
		return "", RetryError{After: 0}
	}
	defer res.Body.Close()
	var out struct {
		APNsID     string `json:"apnsId"`
		Reason     string `json:"reason"`
		RetryAfter int    `json:"retryAfter"`
	}
	json.NewDecoder(io.LimitReader(res.Body, 4096)).Decode(&out)
	switch res.StatusCode {
	case http.StatusOK:
		return out.APNsID, nil
	case http.StatusGone:
		return "", ErrGone
	case http.StatusTooManyRequests:
		return "", ErrLimited
	case http.StatusServiceUnavailable:
		return "", RetryError{After: time.Duration(out.RetryAfter) * time.Second}
	}
	return "", fmt.Errorf("push: relay %s %s", res.Status, out.Reason)
}
