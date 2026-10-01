package main

import (
	"errors"
	"strings"
	"testing"

	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/launch"
)

func TestAnUnknownConfigKeyIsNamedAsUnknown(t *testing.T) {
	settings := config.NewSettings(t.TempDir())
	set := configSetter(launch.New(&daemon.Daemon{}, nil, settings, nil), settings)
	err := set("phone.maxAcess", "ask")
	if err == nil || errors.Is(err, config.ErrInvalid) || !strings.Contains(err.Error(), `unknown key "phone.maxAcess"`) {
		t.Fatalf("err = %v", err)
	}
}
