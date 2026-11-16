package main

import (
	"context"
	"log"

	"pocketd/internal/config"
	"pocketd/internal/hosts"
	"pocketd/internal/wsserver"
)

// openHosts links the hosts saved in home's state/hosts.json until ctx ends; nil serves none.
func openHosts(ctx context.Context, home string, env func() []string, name string) *wsserver.Hosts {
	m, err := hosts.Open(hosts.Paths{Home: home, RemoteDir: config.HomeName()}, env, versionString(), name)
	if err != nil {
		log.Printf("hosts: %v; serving none", err)
		return nil
	}
	m.Start(ctx)
	return &wsserver.Hosts{Manager: m}
}
