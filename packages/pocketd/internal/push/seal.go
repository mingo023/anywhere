package push

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"errors"
)

var errSealed = errors.New("push: not a sealed box")

// Seal encrypts plain under key with AES-256-GCM and aad as associated data,
// as base64 of nonce(12) ‖ ciphertext ‖ tag(16): CryptoKit's
// AES.GCM.SealedBox(combined:) layout, which the phone opens.
func Seal(key []byte, aad string, plain []byte) (string, error) {
	nonce := make([]byte, 12)
	if _, err := rand.Read(nonce); err != nil {
		return "", err
	}
	return sealWith(key, nonce, aad, plain)
}

func sealWith(key, nonce []byte, aad string, plain []byte) (string, error) {
	gcm, err := newGCM(key)
	if err != nil {
		return "", err
	}
	return base64.StdEncoding.EncodeToString(gcm.Seal(nonce, nonce, plain, []byte(aad))), nil
}

// Unseal is Seal's inverse.
func Unseal(key []byte, aad, sealed string) ([]byte, error) {
	gcm, err := newGCM(key)
	if err != nil {
		return nil, err
	}
	raw, err := base64.StdEncoding.DecodeString(sealed)
	if err != nil || len(raw) < 12+gcm.Overhead() {
		return nil, errSealed
	}
	return gcm.Open(nil, raw[:12], raw[12:], []byte(aad))
}

func newGCM(key []byte) (cipher.AEAD, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, err
	}
	return cipher.NewGCM(block)
}

// Opaque is a stable id for agent that only key's holder can link to it: the
// thread and collapse id of its alerts, so the relay and Apple see no agent ids.
func Opaque(key []byte, agent string) string {
	m := hmac.New(sha256.New, key)
	m.Write([]byte("thread:" + agent))
	return base64.RawURLEncoding.EncodeToString(m.Sum(nil))[:16]
}
