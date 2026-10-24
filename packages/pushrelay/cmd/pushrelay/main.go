// Command pushrelay holds the APNs key and pushes only to handles a phone sealed itself.
//
// Config, from the environment:
//
//	APNS_KEY_P8    path to the .p8 key Apple issued
//	APNS_KEY_ID    its 10-character key id
//	APNS_TEAM_ID   the team id
//	BUNDLE_ID      the app's bundle id (default dev.mingo.anywhere)
//	SEAL_KEY       base64 of 32 random bytes; seals handles
//	MAC_KEY        base64 of 32 random bytes; derives each handle's secret
//	LISTEN         address to serve on (default :8080)
//	CLIENT_IP      header a proxy puts the caller's address in (Fly-Client-IP on Fly.io); unset takes the socket's
package main

import (
	"cmp"
	"encoding/base64"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os"
	"time"

	"pushrelay/internal/apns"
	"pushrelay/internal/handle"
	"pushrelay/internal/relay"
)

func main() {
	srv, listen, err := configure(os.Getenv)
	if err != nil {
		fmt.Fprintln(os.Stderr, "pushrelay:", err)
		os.Exit(2)
	}
	log.Printf("pushrelay serving %s on %s", srv.Bundle, listen)
	h := &http.Server{Addr: listen, Handler: srv.Handler(), ReadHeaderTimeout: 10 * time.Second, ReadTimeout: 30 * time.Second}
	log.Fatal(h.ListenAndServe())
}

func configure(env func(string) string) (*relay.Server, string, error) {
	p8, err := os.ReadFile(env("APNS_KEY_P8"))
	if err != nil {
		return nil, "", fmt.Errorf("APNS_KEY_P8: %w", err)
	}
	key, err := apns.ParseKey(p8)
	if err != nil {
		return nil, "", fmt.Errorf("APNS_KEY_P8: %w", err)
	}
	keyID, teamID := env("APNS_KEY_ID"), env("APNS_TEAM_ID")
	if keyID == "" || teamID == "" {
		return nil, "", errors.New("APNS_KEY_ID and APNS_TEAM_ID are required")
	}
	seal, err1 := secret(env("SEAL_KEY"))
	mac, err2 := secret(env("MAC_KEY"))
	if err := errors.Join(err1, err2); err != nil {
		return nil, "", fmt.Errorf("SEAL_KEY and MAC_KEY: %w", err)
	}
	sealer, err := handle.NewSealer(seal, mac)
	if err != nil {
		return nil, "", err
	}
	srv := &relay.Server{
		Bundle:   cmp.Or(env("BUNDLE_ID"), "dev.mingo.anywhere"),
		Sealer:   sealer,
		APNs:     &apns.Client{KeyID: keyID, TeamID: teamID, Key: key, HTTP: apns.NewHTTP()},
		ClientIP: env("CLIENT_IP"),
		Log:      log.Default(),
	}
	return srv, cmp.Or(env("LISTEN"), ":8080"), nil
}

func secret(b64 string) ([]byte, error) {
	b, err := base64.StdEncoding.DecodeString(b64)
	if err != nil || len(b) != 32 {
		return nil, errors.New("want base64 of 32 bytes (openssl rand -base64 32)")
	}
	return b, nil
}
