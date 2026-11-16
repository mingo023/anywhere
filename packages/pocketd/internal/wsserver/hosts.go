package wsserver

import (
	"context"
	"errors"
	"slices"
	"sync"

	"pocketd/internal/hosts"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

type Hosts struct {
	*hosts.Manager
	mu    sync.Mutex
	feeds map[*conn]bool
}

type hostsConn struct {
	// mu keeps a verb's reply ahead of the pushes it causes.
	mu   sync.Mutex
	stop func()
	last []proto.Host
}

// hostsFeed keeps the owner's desktop current: the list on hello, then each host as it changes.
func (c *conn) hostsFeed(caps []string) {
	hs := c.s.Hosts
	if hs == nil || !c.who.Has(peer.Own) || !slices.Contains(caps, proto.CapHosts) {
		return
	}
	if c.hosts.stop != nil {
		c.pushHosts(true)
		return
	}
	ctx, cancel := context.WithCancel(c.ctx)
	c.hosts.stop = cancel
	hs.mu.Lock()
	if hs.feeds == nil {
		hs.feeds = map[*conn]bool{}
	}
	hs.feeds[c] = true
	hs.mu.Unlock()
	go func() {
		for full := true; ; full = false {
			changed := hs.Changed()
			c.pushHosts(full)
			select {
			case <-ctx.Done():
				return
			case <-changed:
			}
		}
	}()
}

func (c *conn) closeHosts() {
	if c.hosts.stop == nil {
		return
	}
	c.hosts.stop()
	c.s.Hosts.mu.Lock()
	delete(c.s.Hosts.feeds, c)
	c.s.Hosts.mu.Unlock()
}

func (c *conn) pushHosts(full bool) {
	c.hosts.mu.Lock()
	defer c.hosts.mu.Unlock()
	now := wireHosts(c.s.Hosts.List())
	last := c.hosts.last
	c.hosts.last = now
	removed := slices.ContainsFunc(last, func(h proto.Host) bool {
		return !slices.ContainsFunc(now, func(n proto.Host) bool { return n.ID == h.ID })
	})
	if full || removed {
		c.send(proto.NewHostList("", now))
		return
	}
	for _, h := range now {
		if !slices.Contains(last, h) {
			c.send(proto.NewHostUpdate("", h))
		}
	}
}

func (c *conn) host(m proto.ClientMessage) error {
	hs := c.s.Hosts
	if hs == nil || !slices.Contains(c.caps, proto.CapHosts) {
		return proto.ErrMalformed
	}
	switch {
	case m.Type == "host.list":
		c.send(proto.NewHostList(m.ID, wireHosts(hs.List())))
		return nil
	case m.Type == "host.sign_in":
		go func() { c.hostReply(m, hs.SignIn(m.HostID, m.Password)) }()
		return nil
	case m.Type == "host.remove" && m.Uninstall:
		go func() { c.hostReply(m, hs.Remove(m.HostID, true)) }()
		return nil
	case m.Type == "host.add":
		c.hosts.mu.Lock()
		defer c.hosts.mu.Unlock()
		e, err := hs.Add(hosts.Target(*m.HostTarget), m.Name)
		if err != nil {
			c.send(hostError(m, hs, err))
			return nil
		}
		c.send(proto.NewHostUpdate(m.ID, wireHost(e)))
		return nil
	}
	c.hosts.mu.Lock()
	defer c.hosts.mu.Unlock()
	var err error
	switch m.Type {
	case "host.remove":
		err = hs.Remove(m.HostID, false)
	case "host.retry":
		err = hs.Retry(m.HostID)
	case "host.install":
		err = hs.Install(m.HostID, m.Update, m.ArchiveDir, hs.progress(m.HostID, m.Update))
	}
	c.reply(m, hs, err)
	return nil
}

func (c *conn) hostReply(m proto.ClientMessage, err error) {
	c.hosts.mu.Lock()
	defer c.hosts.mu.Unlock()
	c.reply(m, c.s.Hosts, err)
}

func (c *conn) reply(m proto.ClientMessage, hs *Hosts, err error) {
	if err != nil {
		c.send(hostError(m, hs, err))
		return
	}
	c.send(proto.NewAck(m.ID))
}

func (hs *Hosts) progress(id string, update bool) func(hosts.Progress) {
	return func(p hosts.Progress) {
		msg := proto.NewHostProgress(id, update, proto.InstallProgress{Kind: string(p.Kind), Step: string(p.Step), Text: p.Text})
		hs.mu.Lock()
		var feeds []*conn
		for c := range hs.feeds {
			feeds = append(feeds, c)
		}
		hs.mu.Unlock()
		for _, c := range feeds {
			c.hosts.mu.Lock()
			c.send(msg)
			c.hosts.mu.Unlock()
		}
	}
}

func hostError(m proto.ClientMessage, hs *Hosts, err error) proto.Error {
	name := m.HostID
	for _, e := range hs.List() {
		if e.ID == m.HostID {
			name = e.Name
		}
	}
	var exists hosts.ExistsError
	var invalid hosts.InvalidError
	var ssh *hosts.SSHError
	switch {
	case errors.Is(err, hosts.ErrUnknown):
		return proto.NewCodedError(m.ID, proto.CodeUnknownHost, err.Error(), "")
	case errors.As(err, &exists):
		return proto.NewCodedError(m.ID, proto.CodeHostExists, err.Error(), "")
	case errors.As(err, &invalid):
		return proto.NewCodedError(m.ID, proto.CodeInvalidHost, err.Error(), "")
	case errors.Is(err, hosts.ErrBusy):
		return proto.NewCodedError(m.ID, proto.CodeHostBusy, err.Error(), "")
	case errors.As(err, &ssh) && m.Type == "host.sign_in" && ssh.Failure == hosts.FailAuth:
		return proto.NewCodedError(m.ID, proto.CodeHostSSH, "Wrong password.", string(ssh.Failure))
	case errors.As(err, &ssh):
		return proto.NewCodedError(m.ID, proto.CodeHostSSH, ssh.Failure.Message(name), string(ssh.Failure))
	}
	return proto.NewError(m.ID, err.Error())
}

func wireHosts(entries []hosts.Entry) []proto.Host {
	out := make([]proto.Host, len(entries))
	for i, e := range entries {
		out[i] = wireHost(e)
	}
	return out
}

func wireHost(e hosts.Entry) proto.Host {
	return proto.Host{ID: e.ID, Name: e.Name, Target: proto.HostTarget(e.Target), Windows: e.Windows, Socket: e.Socket,
		Link: proto.HostLinkState{State: string(e.Link.State), Version: e.Link.Version, Uptime: e.Link.Uptime, Failure: string(e.Link.Failure)}}
}
