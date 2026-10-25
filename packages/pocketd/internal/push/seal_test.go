package push

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"testing"
)

var update = flag.Bool("update", false, "rewrite testdata")

// golden compares got with testdata/name, or writes it with -update. The
// phone's tests read the same files.
func golden(t *testing.T, name string, got []byte) {
	t.Helper()
	path := filepath.Join("testdata", name)
	if *update {
		os.MkdirAll("testdata", 0o755)
		if err := os.WriteFile(path, append(bytes.TrimSpace(got), '\n'), 0o644); err != nil {
			t.Fatal(err)
		}
		return
	}
	want, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("%v (run with -update)", err)
	}
	if !bytes.Equal(bytes.TrimSpace(want), bytes.TrimSpace(got)) {
		t.Fatalf("%s:\n got %s\nwant %s", name, got, want)
	}
}

// vector is a sealed notice with every input fixed, which the phone's CryptoKit must open.
type vector struct {
	Key    string `json:"key"`
	Nonce  string `json:"nonce"`
	AAD    string `json:"aad"`
	Plain  string `json:"plain"`
	Sealed string `json:"sealed"`
}

func TestASealedNoticeMatchesTheVectorThePhoneOpens(t *testing.T) {
	key := bytes.Repeat([]byte{0x42}, 32)
	nonce := []byte("0123456789ab")
	plain, _ := json.Marshal(Notice{Mac: "studio", Agent: "a1", Kind: NeedsYou, Title: "Fix the flaky test", Place: "anywhere · calm-otter", Line: "Wants to run npm test", Request: "r1", Badge: 2})
	sealed, err := sealWith(key, nonce, "studio", plain)
	if err != nil {
		t.Fatal(err)
	}
	raw, _ := json.MarshalIndent(vector{base64.StdEncoding.EncodeToString(key), base64.StdEncoding.EncodeToString(nonce), "studio", string(plain), sealed}, "", "  ")
	golden(t, "seal_vector.json", raw)
	box, _ := base64.StdEncoding.DecodeString(sealed)
	if !bytes.Equal(box[:12], nonce) || len(box) != 12+len(plain)+16 {
		t.Fatalf("layout: %d bytes", len(box))
	}
}

func TestASealedBoxOpensOnlyWithItsKeyAndMac(t *testing.T) {
	key := bytes.Repeat([]byte{1}, 32)
	sealed, err := Seal(key, "studio", []byte("hi"))
	if err != nil {
		t.Fatal(err)
	}
	if plain, err := Unseal(key, "studio", sealed); err != nil || string(plain) != "hi" {
		t.Fatalf("%q %v", plain, err)
	}
	if _, err := Unseal(key, "laptop", sealed); err == nil {
		t.Fatal("opened for another Mac")
	}
	if _, err := Unseal(bytes.Repeat([]byte{2}, 32), "studio", sealed); err == nil {
		t.Fatal("opened under another key")
	}
	if again, _ := Seal(key, "studio", []byte("hi")); again == sealed {
		t.Fatal("two seals share a nonce")
	}
}

func TestAnAgentsOpaqueIdIsStablePerKeyAndNamesNoAgent(t *testing.T) {
	k1, k2 := bytes.Repeat([]byte{1}, 32), bytes.Repeat([]byte{2}, 32)
	a := Opaque(k1, "agent-1")
	if a != Opaque(k1, "agent-1") || a == Opaque(k1, "agent-2") || a == Opaque(k2, "agent-1") || len(a) != 16 || bytes.Contains([]byte(a), []byte("agent")) {
		t.Fatalf("%q", a)
	}
}
