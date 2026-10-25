package apns

import (
	"context"
	"crypto/ecdh"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"io"
	"math/big"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func p8(t *testing.T) ([]byte, *ecdsa.PrivateKey) {
	k, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	der, _ := x509.MarshalPKCS8PrivateKey(k)
	return pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: der}), k
}

// verify checks a JWT the way APNs does: header, claims and an ES256 r‖s signature under pub.
func verify(t *testing.T, jwt string, pub *ecdsa.PublicKey) (head, claims map[string]any) {
	t.Helper()
	parts := strings.Split(jwt, ".")
	if len(parts) != 3 {
		t.Fatalf("%d parts", len(parts))
	}
	enc := base64.RawURLEncoding
	for i, dst := range []*map[string]any{&head, &claims} {
		raw, err := enc.DecodeString(parts[i])
		if err != nil || json.Unmarshal(raw, dst) != nil {
			t.Fatalf("part %d: %q", i, raw)
		}
	}
	sig, err := enc.DecodeString(parts[2])
	if err != nil || len(sig) != 64 {
		t.Fatalf("signature is %d bytes", len(sig))
	}
	sum := sha256.Sum256([]byte(parts[0] + "." + parts[1]))
	if !ecdsa.Verify(pub, sum[:], new(big.Int).SetBytes(sig[:32]), new(big.Int).SetBytes(sig[32:])) {
		t.Fatal("signature doesn't verify under the public key")
	}
	return head, claims
}

func TestTheJWTIsES256WithRawSignatureVerifiableWithThePublicKey(t *testing.T) {
	pem, key := p8(t)
	parsed, err := ParseKey(pem)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Unix(1761300000, 0)
	c := &Client{KeyID: "ABC123DEFG", TeamID: "TEAM123456", Key: parsed, Now: func() time.Time { return now }}
	jwt, err := c.Token()
	if err != nil {
		t.Fatal(err)
	}
	head, claims := verify(t, jwt, &key.PublicKey)
	if head["alg"] != "ES256" || head["kid"] != "ABC123DEFG" || len(head) != 2 {
		t.Fatalf("%v", head)
	}
	if claims["iss"] != "TEAM123456" || claims["iat"] != float64(1761300000) || len(claims) != 2 {
		t.Fatalf("%v", claims)
	}
}

func TestTheJWTIsReusedForFortyMinutesThenSignedAnew(t *testing.T) {
	_, key := p8(t)
	now := time.Unix(1761300000, 0)
	c := &Client{KeyID: "K", TeamID: "T", Key: key, Now: func() time.Time { return now }}
	first, _ := c.Token()
	now = now.Add(39 * time.Minute)
	if again, _ := c.Token(); again != first {
		t.Fatal("re-signed before 40 minutes")
	}
	now = now.Add(time.Minute)
	if later, _ := c.Token(); later == first {
		t.Fatal("still the old token at 40 minutes")
	}
}

func TestAKeyThatIsNotAP8IsRefused(t *testing.T) {
	x, _ := ecdh.X25519().GenerateKey(rand.Reader)
	der, _ := x509.MarshalPKCS8PrivateKey(x)
	for _, bad := range [][]byte{[]byte("nope"), pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: der})} {
		if _, err := ParseKey(bad); err == nil {
			t.Errorf("%q parsed", bad)
		}
	}
}

func TestASendIsOneHTTP2PostWithTheHeadersAPNsReads(t *testing.T) {
	_, key := p8(t)
	type seen struct {
		proto  int
		path   string
		header http.Header
		body   string
	}
	got := make(chan seen, 1)
	srv := httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		got <- seen{r.ProtoMajor, r.URL.Path, r.Header.Clone(), string(body)}
		w.Header().Set("apns-id", r.Header.Get("apns-id"))
	}))
	srv.EnableHTTP2 = true
	srv.StartTLS()
	defer srv.Close()
	c := &Client{KeyID: "K", TeamID: "T", Key: key, HTTP: srv.Client(), Hosts: map[string]string{"development": srv.URL}}
	res, err := c.Send(context.Background(), Request{Token: "beef", Env: "development", Topic: "dev.mingo.anywhere", PushType: "alert",
		Priority: 10, Expiration: 1761303600, CollapseID: "c1", Payload: []byte(`{"aps":{}}`)})
	if err != nil || res.Status != 200 {
		t.Fatalf("%+v %v", res, err)
	}
	s := <-got
	h := s.header
	if s.proto != 2 || s.path != "/3/device/beef" || s.body != `{"aps":{}}` {
		t.Fatalf("%+v", s)
	}
	if h.Get("apns-topic") != "dev.mingo.anywhere" || h.Get("apns-push-type") != "alert" || h.Get("apns-priority") != "10" ||
		h.Get("apns-expiration") != "1761303600" || h.Get("apns-collapse-id") != "c1" || !strings.HasPrefix(h.Get("authorization"), "bearer ") {
		t.Fatalf("%v", h)
	}
	if id := h.Get("apns-id"); len(id) != 36 || id[14] != '4' || strings.ToLower(id) != id || res.ID != id {
		t.Fatalf("apns-id %q, echoed %q", id, res.ID)
	}
}

func TestAnExpiredProviderTokenIsSignedAnewOnTheNextSend(t *testing.T) {
	_, key := p8(t)
	var auths []string
	srv := httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		auths = append(auths, r.Header.Get("authorization"))
		if len(auths) == 1 {
			w.WriteHeader(http.StatusForbidden)
			w.Write([]byte(`{"reason":"ExpiredProviderToken"}`))
		}
	}))
	srv.EnableHTTP2 = true
	srv.StartTLS()
	defer srv.Close()
	now := time.Unix(1761300000, 0)
	c := &Client{KeyID: "K", TeamID: "T", Key: key, HTTP: srv.Client(), Hosts: map[string]string{"production": srv.URL}, Now: func() time.Time { return now }}
	res, _ := c.Send(context.Background(), Request{Token: "t", Env: "production", Priority: 10, Payload: []byte(`{}`)})
	if res.Status != 403 || res.Reason != "ExpiredProviderToken" {
		t.Fatalf("%+v", res)
	}
	now = now.Add(time.Second)
	c.Send(context.Background(), Request{Token: "t", Env: "production", Priority: 10, Payload: []byte(`{}`)})
	if len(auths) != 2 || auths[0] == auths[1] {
		t.Fatalf("%d sends, same token %v", len(auths), auths[0] == auths[1])
	}
}
