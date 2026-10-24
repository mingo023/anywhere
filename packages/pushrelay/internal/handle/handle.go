// Package handle seals an APNs token into an opaque handle only this relay
// can open, and derives the secret pocketd signs each send with.
package handle

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
)

// Kinds a handle can push as.
const (
	Alert    = "alert"
	Activity = "activity"
	Start    = "start"
)

// Envs a token was minted in.
const (
	Development = "development"
	Production  = "production"
)

var (
	ErrInvalid = errors.New("handle: invalid")
	ErrKey     = errors.New("handle: keys are 32 bytes each")
)

// Token is what a handle seals: the APNs token, the kind it pushes as, its env and when it expires (unix seconds).
type Token struct {
	V     int    `json:"v"`
	Token string `json:"t"`
	Kind  string `json:"k"`
	Env   string `json:"e"`
	Exp   int64  `json:"x"`
}

// Sealer holds K_seal and K_mac.
type Sealer struct {
	aead cipher.AEAD
	mac  []byte
}

func NewSealer(sealKey, macKey []byte) (*Sealer, error) {
	if len(sealKey) != 32 || len(macKey) != 32 {
		return nil, ErrKey
	}
	block, err := aes.NewCipher(sealKey)
	if err != nil {
		return nil, err
	}
	aead, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}
	return &Sealer{aead: aead, mac: append([]byte(nil), macKey...)}, nil
}

var enc = base64.RawURLEncoding

// Seal returns b64url(nonce12 ‖ AES-GCM(K_seal, t)).
func (s *Sealer) Seal(t Token) (string, error) {
	t.V = 1
	plain, err := json.Marshal(t)
	if err != nil {
		return "", err
	}
	nonce := make([]byte, s.aead.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return "", err
	}
	return enc.EncodeToString(s.aead.Seal(nonce, nonce, plain, nil)), nil
}

// Open unseals a handle; any tampering, or a handle from other keys, is ErrInvalid.
func (s *Sealer) Open(h string) (Token, error) {
	raw, err := enc.DecodeString(h)
	n := s.aead.NonceSize()
	if err != nil || len(raw) < n+s.aead.Overhead() {
		return Token{}, ErrInvalid
	}
	plain, err := s.aead.Open(nil, raw[:n], raw[n:], nil)
	if err != nil {
		return Token{}, ErrInvalid
	}
	var t Token
	if json.Unmarshal(plain, &t) != nil || t.V != 1 {
		return Token{}, ErrInvalid
	}
	return t, nil
}

// Secret is HMAC-SHA256(K_mac, handle): the key pocketd signs this handle's sends with.
func (s *Sealer) Secret(h string) []byte {
	m := hmac.New(sha256.New, s.mac)
	m.Write([]byte(h))
	return m.Sum(nil)
}

// SecretString is Secret as the phone receives it, b64url.
func (s *Sealer) SecretString(h string) string { return enc.EncodeToString(s.Secret(h)) }

// Sign is what a sender puts in X-Anywhere-Signature: b64url(HMAC-SHA256(secret, ts + "." + body)).
func Sign(secret []byte, ts string, body []byte) string {
	return enc.EncodeToString(signature(secret, ts, body))
}

// Verify checks a signature in constant time.
func Verify(secret []byte, ts string, body []byte, sig string) bool {
	got, err := enc.DecodeString(sig)
	return err == nil && hmac.Equal(got, signature(secret, ts, body))
}

func signature(secret []byte, ts string, body []byte) []byte {
	m := hmac.New(sha256.New, secret)
	m.Write([]byte(ts))
	m.Write([]byte("."))
	m.Write(body)
	return m.Sum(nil)
}
