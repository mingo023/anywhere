// Package apns posts to Apple's push service over HTTP/2 with a token (JWT) it caches.
package apns

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"crypto/rand"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"encoding/pem"
	"errors"
	"io"
	"net/http"
	"strconv"
	"sync"
	"time"
)

// Hosts are APNs by env.
var Hosts = map[string]string{
	"production":  "https://api.push.apple.com",
	"development": "https://api.sandbox.push.apple.com",
}

// TokenAge is how long a JWT is reused. Apple refuses one older than an hour
// and throttles refreshing more often than every 20 minutes.
const TokenAge = 40 * time.Minute

// ParseKey reads the .p8 Apple issues: a PEM PKCS#8 P-256 key.
func ParseKey(p8 []byte) (*ecdsa.PrivateKey, error) {
	block, _ := pem.Decode(p8)
	if block == nil {
		return nil, errors.New("apns: the key isn't PEM")
	}
	k, err := x509.ParsePKCS8PrivateKey(block.Bytes)
	if err != nil {
		return nil, err
	}
	ec, ok := k.(*ecdsa.PrivateKey)
	if !ok {
		return nil, errors.New("apns: the key isn't an EC key")
	}
	return ec, nil
}

// NewHTTP is one long-lived client; its connections are reused for hours.
func NewHTTP() *http.Client {
	return &http.Client{Timeout: 30 * time.Second, Transport: &http.Transport{
		ForceAttemptHTTP2: true,
		TLSClientConfig:   &tls.Config{MinVersion: tls.VersionTLS12},
		IdleConnTimeout:   time.Hour,
	}}
}

type Client struct {
	KeyID, TeamID string
	Key           *ecdsa.PrivateKey
	// Hosts overrides the package's, for a fake APNs under test.
	Hosts map[string]string
	HTTP  *http.Client
	Now   func() time.Time

	mu    sync.Mutex
	jwt   string
	jwtAt time.Time
}

type Request struct {
	Token, Env      string
	Topic, PushType string
	Priority        int
	Expiration      int64
	CollapseID      string
	Payload         []byte
}

// Response is APNs' answer; Reason is its error reason, RetryAfter its Retry-After in seconds.
type Response struct {
	Status     int
	ID         string
	Reason     string
	RetryAfter int
}

func (c *Client) now() time.Time {
	if c.Now != nil {
		return c.Now()
	}
	return time.Now()
}

// Token is the cached JWT, signed anew once it is TokenAge old.
func (c *Client) Token() (string, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	now := c.now()
	if c.jwt != "" && now.Sub(c.jwtAt) < TokenAge {
		return c.jwt, nil
	}
	t, err := sign(c.Key, c.KeyID, c.TeamID, now)
	if err != nil {
		return "", err
	}
	c.jwt, c.jwtAt = t, now
	return t, nil
}

// forget drops a JWT Apple called expired or invalid, so the next send signs a new one.
func (c *Client) forget(t string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.jwt == t {
		c.jwt = ""
	}
}

// sign makes an ES256 JWT; its signature is raw r‖s, 64 bytes, not ASN.1.
func sign(key *ecdsa.PrivateKey, keyID, teamID string, now time.Time) (string, error) {
	enc := base64.RawURLEncoding
	head, _ := json.Marshal(map[string]string{"alg": "ES256", "kid": keyID})
	claims, _ := json.Marshal(map[string]any{"iss": teamID, "iat": now.Unix()})
	unsigned := enc.EncodeToString(head) + "." + enc.EncodeToString(claims)
	sum := sha256.Sum256([]byte(unsigned))
	r, s, err := ecdsa.Sign(rand.Reader, key, sum[:])
	if err != nil {
		return "", err
	}
	sig := make([]byte, 64)
	r.FillBytes(sig[:32])
	s.FillBytes(sig[32:])
	return unsigned + "." + enc.EncodeToString(sig), nil
}

func (c *Client) host(env string) string {
	if h, ok := c.Hosts[env]; ok {
		return h
	}
	return Hosts[env]
}

// Send posts one push. An error is a transport failure; APNs refusals come back in Response.
func (c *Client) Send(ctx context.Context, r Request) (Response, error) {
	host := c.host(r.Env)
	if host == "" {
		return Response{}, errors.New("apns: unknown env " + r.Env)
	}
	jwt, err := c.Token()
	if err != nil {
		return Response{}, err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, host+"/3/device/"+r.Token, bytes.NewReader(r.Payload))
	if err != nil {
		return Response{}, err
	}
	req.Header.Set("authorization", "bearer "+jwt)
	req.Header.Set("apns-topic", r.Topic)
	req.Header.Set("apns-push-type", r.PushType)
	req.Header.Set("apns-priority", strconv.Itoa(r.Priority))
	req.Header.Set("apns-expiration", strconv.FormatInt(r.Expiration, 10))
	req.Header.Set("apns-id", uuid())
	if r.CollapseID != "" {
		req.Header.Set("apns-collapse-id", r.CollapseID)
	}
	req.Header.Set("content-type", "application/json")
	res, err := c.HTTP.Do(req)
	if err != nil {
		return Response{}, err
	}
	defer res.Body.Close()
	out := Response{Status: res.StatusCode, ID: res.Header.Get("apns-id")}
	if res.StatusCode != http.StatusOK {
		var body struct{ Reason string }
		json.NewDecoder(io.LimitReader(res.Body, 4096)).Decode(&body)
		out.Reason = body.Reason
		out.RetryAfter, _ = strconv.Atoi(res.Header.Get("retry-after"))
		if res.StatusCode == http.StatusForbidden && (body.Reason == "ExpiredProviderToken" || body.Reason == "InvalidProviderToken") {
			c.forget(jwt)
		}
	}
	return out, nil
}

// uuid is a random v4 UUID, lowercase as APNs echoes it.
func uuid() string {
	b := make([]byte, 16)
	rand.Read(b)
	b[6] = b[6]&0x0f | 0x40
	b[8] = b[8]&0x3f | 0x80
	h := hex.EncodeToString(b)
	return h[:8] + "-" + h[8:12] + "-" + h[12:16] + "-" + h[16:20] + "-" + h[20:]
}
