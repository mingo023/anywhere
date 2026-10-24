package main

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/x509"
	"encoding/base64"
	"encoding/pem"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestTheRelayConfiguresFromItsEnvironment(t *testing.T) {
	k, _ := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	der, _ := x509.MarshalPKCS8PrivateKey(k)
	path := filepath.Join(t.TempDir(), "AuthKey.p8")
	os.WriteFile(path, pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: der}), 0o600)
	key := base64.StdEncoding.EncodeToString(make([]byte, 32))
	vars := map[string]string{"APNS_KEY_P8": path, "APNS_KEY_ID": "K", "APNS_TEAM_ID": "T", "SEAL_KEY": key, "MAC_KEY": key}
	srv, listen, err := configure(func(k string) string { return vars[k] })
	if err != nil || listen != ":8080" || srv.Bundle != "dev.mingo.anywhere" {
		t.Fatalf("%v %q", err, listen)
	}
	vars["MAC_KEY"] = "short"
	if _, _, err := configure(func(k string) string { return vars[k] }); err == nil || !strings.Contains(err.Error(), "MAC_KEY") {
		t.Fatalf("%v", err)
	}
}
