package main

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"strconv"
	"strings"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/hub"
	"pocketd/internal/ops"
	"pocketd/internal/terminal"
	"pocketd/internal/wsserver"
)

func serve(sock string) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	h := hub.New()
	d := &daemon.Daemon{
		Terminals: terminal.NewManager(),
		Agents:    agent.NewRegistry(h),
		Broker:    broker.New(h),
		Home:      config.Home(),
		Exe:       exe,
		Sock:      sock,
	}
	if err := d.WritePlugin(); err != nil {
		return err
	}
	d.Terminals.OnInput = d.Input
	host, _ := os.Hostname()
	phones, err := net.Listen("tcp", ":"+strconv.Itoa(cfg.Port))
	if err != nil {
		return err
	}
	go http.Serve(phones, &wsserver.Server{Token: cfg.Token, Hostname: host, Agents: d.Agents, Broker: d.Broker, Hub: h})
	go d.Watch(context.Background())

	ln, err := ops.Listen(sock)
	if err != nil {
		return err
	}
	fmt.Println("pocketd listening on", sock)
	fmt.Printf("phone: ws://%s:%d\n", tailscaleIP(), cfg.Port)
	fmt.Println("token:", cfg.Token)
	return (&ops.Server{Terminals: d.Terminals, Spawn: d.Spawn, Hook: d.Hook}).Serve(ln)
}

func tailscaleIP() string {
	out, err := exec.Command("tailscale", "ip", "-4").Output()
	if ip, _, _ := strings.Cut(strings.TrimSpace(string(out)), "\n"); err == nil && ip != "" {
		return ip
	}
	return "localhost"
}
