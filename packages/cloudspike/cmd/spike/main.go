// Command spike runs the cloud spike's relay, tunnels a real TCP service
// through it, and loads it to measure what one link costs.
//
//	spike relay -listen 127.0.0.1:8787
//	spike demo  -relay ws://127.0.0.1:8787 -target 127.0.0.1:<pocketd port> -listen 127.0.0.1:9000
//	spike bench -relay ws://127.0.0.1:8787 -links 200 -idle 1000 -dur 60s -pid <relay pid>
package main

import (
	"context"
	"crypto/ed25519"
	"encoding/hex"
	"flag"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"cloudspike/channel"
	"cloudspike/members"
	"cloudspike/relay"
)

func main() {
	if len(os.Args) < 2 {
		log.Fatal("usage: spike relay|demo|bench [flags]")
	}
	fs := flag.NewFlagSet(os.Args[1], flag.ExitOnError)
	var err error
	switch os.Args[1] {
	case "relay":
		listen := fs.String("listen", "127.0.0.1:8787", "address to serve on")
		fs.Parse(os.Args[2:])
		err = runRelay(*listen)
	case "demo":
		base := fs.String("relay", "ws://127.0.0.1:8787", "relay URL")
		target := fs.String("target", "", "TCP service on the machine, e.g. pocketd's owner port")
		listen := fs.String("listen", "127.0.0.1:9000", "where the member side listens")
		fs.Parse(os.Args[2:])
		err = runDemo(*base, *target, *listen)
	case "bench":
		base := fs.String("relay", "ws://127.0.0.1:8787", "relay URL")
		links := fs.Int("links", 100, "busy member-to-machine links")
		idle := fs.Int("idle", 0, "machines holding only their relay line")
		dur := fs.Duration("dur", 60*time.Second, "how long to run")
		pid := fs.Int("pid", 0, "relay process to sample with ps")
		down := fs.Int("down", 16600, "bytes/s machine to member (measured 2026-11-25)")
		up := fs.Int("up", 4600, "bytes/s member to machine (measured 2026-11-25)")
		fs.Parse(os.Args[2:])
		err = runBench(*base, *links, *idle, *dur, *pid, *down, *up)
	default:
		err = fmt.Errorf("unknown command %q", os.Args[1])
	}
	if err != nil {
		log.Fatal(err)
	}
}

func runRelay(listen string) error {
	r := relay.New()
	go func() {
		for range time.Tick(30 * time.Second) {
			var total int64
			for _, n := range r.Usage() {
				total += n
			}
			log.Printf("forwarded %d bytes across %d accounts", total, len(r.Usage()))
		}
	}()
	log.Printf("relay on %s", listen)
	srv := &http.Server{Addr: listen, Handler: r, ReadHeaderTimeout: 10 * time.Second}
	return srv.ListenAndServe()
}

type device struct {
	key ed25519.PrivateKey
	m   members.Member
}

func newDevice(kind members.Kind, name string) device {
	pub, priv, _ := ed25519.GenerateKey(nil)
	return device{priv, members.Member{Key: pub, Kind: kind, Name: name}}
}

func pair() (mac, host device, mv, hv *members.View) {
	mac, host = newDevice(members.Desktop, "desktop"), newDevice(members.Machine, "machine")
	l := members.Genesis(mac.key, mac.m, nil).Add(mac.key, host.m)
	mv, _ = members.NewView(l)
	hv, _ = members.NewView(l)
	return
}

func serveHost(ctx context.Context, base, account string, host device, v *members.View, handle func(*channel.Conn)) {
	id := hex.EncodeToString(host.m.Key)
	wait := time.Second
	for ctx.Err() == nil {
		err := relay.Serve(ctx, base, account, id, func(raw net.Conn) {
			c, err := channel.Server(raw, host.key, v)
			if err != nil {
				raw.Close()
				return
			}
			defer c.Close()
			handle(c)
		})
		if ctx.Err() != nil {
			return
		}
		log.Printf("relay line: %v; again in %s", err, wait)
		time.Sleep(wait)
		wait = min(2*wait, time.Minute)
	}
}

// dialWhenOnline waits for the machine's relay line, which a fresh process opens a moment after the member side starts.
func dialWhenOnline(ctx context.Context, base, account string, mac device, v *members.View, host ed25519.PublicKey) (c *channel.Conn, err error) {
	for range 100 {
		if c, err = dialHost(ctx, base, account, mac, v, host); err != relay.ErrOffline {
			return c, err
		}
		time.Sleep(100 * time.Millisecond)
	}
	return nil, err
}

func dialHost(ctx context.Context, base, account string, mac device, v *members.View, host ed25519.PublicKey) (*channel.Conn, error) {
	raw, err := relay.Dial(ctx, base, account, hex.EncodeToString(host))
	if err != nil {
		return nil, err
	}
	c, err := channel.Client(raw, mac.key, v, host)
	if err != nil {
		raw.Close()
	}
	return c, err
}

func proxy(a, b net.Conn) {
	var once sync.Once
	done := make(chan struct{})
	cp := func(dst, src net.Conn) {
		io.Copy(dst, src)
		once.Do(func() { close(done) })
	}
	go cp(a, b)
	go cp(b, a)
	<-done
	a.Close()
	b.Close()
}

func runDemo(base, target, listen string) error {
	if target == "" {
		return fmt.Errorf("-target required")
	}
	ctx := context.Background()
	mac, host, mv, hv := pair()
	go serveHost(ctx, base, "demo", host, hv, func(c *channel.Conn) {
		t, err := net.Dial("tcp", target)
		if err != nil {
			log.Printf("target: %v", err)
			return
		}
		proxy(c, t)
	})
	ln, err := net.Listen("tcp", listen)
	if err != nil {
		return err
	}
	log.Printf("member side on %s -> relay %s -> machine -> %s", ln.Addr(), base, target)
	for {
		local, err := ln.Accept()
		if err != nil {
			return err
		}
		go func() {
			c, err := dialWhenOnline(ctx, base, "demo", mac, mv, host.m.Key)
			if err != nil {
				log.Printf("dial: %v", err)
				local.Close()
				return
			}
			proxy(local, c)
		}()
	}
}

// pace writes in ten chunks a second, as the host link's small JSON frames do.
func pace(ctx context.Context, w io.Writer, rate int, n *atomic.Int64) {
	chunk := make([]byte, rate/10)
	t := time.NewTicker(100 * time.Millisecond)
	defer t.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
			if _, err := w.Write(chunk); err != nil {
				return
			}
			n.Add(int64(len(chunk)))
		}
	}
}

func runBench(base string, links, idle int, dur time.Duration, pid, down, up int) error {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	var downN, upN, open atomic.Int64
	for i := range idle {
		_, host, _, hv := pair()
		go serveHost(ctx, base, fmt.Sprintf("idle-%d", i), host, hv, func(*channel.Conn) {})
	}
	var wg sync.WaitGroup
	for i := range links {
		account := fmt.Sprintf("busy-%d", i)
		mac, host, mv, hv := pair()
		go serveHost(ctx, base, account, host, hv, func(c *channel.Conn) {
			go io.Copy(io.Discard, c)
			pace(ctx, c, down, &downN)
		})
		wg.Add(1)
		go func() {
			defer wg.Done()
			c, err := dialWhenOnline(ctx, base, account, mac, mv, host.m.Key)
			if err != nil {
				log.Printf("link %d: %v", i, err)
				return
			}
			open.Add(1)
			go io.Copy(io.Discard, c)
			pace(ctx, c, up, &upN)
		}()
	}
	time.Sleep(5 * time.Second)
	log.Printf("%d of %d links open, %d idle machines", open.Load(), links, idle)
	before, err := sample(pid)
	if err != nil {
		return err
	}
	d0, u0, t0 := downN.Load(), upN.Load(), time.Now()
	time.Sleep(dur)
	after, err := sample(pid)
	if err != nil {
		return err
	}
	wall := time.Since(t0).Seconds()
	fmt.Printf("links=%d idle=%d dur=%.0fs\n", open.Load(), idle, wall)
	fmt.Printf("payload down=%.1f KB/s up=%.1f KB/s (sum over links)\n", float64(downN.Load()-d0)/wall/1000, float64(upN.Load()-u0)/wall/1000)
	if pid != 0 {
		fmt.Printf("relay rss=%.1f MB cpu=%.1f%% of one core\n", float64(after.rssKB)/1024, 100*(after.cpu-before.cpu)/wall)
	}
	cancel()
	wg.Wait()
	return nil
}

type usage struct {
	rssKB int
	cpu   float64
}

func sample(pid int) (usage, error) {
	if pid == 0 {
		return usage{}, nil
	}
	out, err := exec.Command("ps", "-o", "rss=,time=", "-p", strconv.Itoa(pid)).Output()
	if err != nil {
		return usage{}, fmt.Errorf("ps: %w", err)
	}
	f := strings.Fields(string(out))
	if len(f) != 2 {
		return usage{}, fmt.Errorf("ps: %q", out)
	}
	rss, _ := strconv.Atoi(f[0])
	return usage{rss, cpuSeconds(f[1])}, nil
}

func cpuSeconds(s string) float64 {
	var total float64
	for _, part := range strings.Split(s, ":") {
		v, _ := strconv.ParseFloat(part, 64)
		total = total*60 + v
	}
	return total
}
