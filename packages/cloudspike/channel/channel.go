// Package channel opens the encrypted link between two members: TLS 1.3 with
// each end's member key as its certificate, checked against the member list
// instead of a CA. The relay in between sees only TLS records.
package channel

import (
	"crypto/ed25519"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/big"
	"net"
	"time"

	"cloudspike/members"
)

var ErrWrongPeer = errors.New("peer is not the member asked for")

// Conn carries the bytes of the protocol pocketd speaks today, unchanged.
type Conn struct {
	*tls.Conn
	Peer members.Member
	stop func()
}

func (c *Conn) Close() error {
	c.stop()
	return c.Conn.Close()
}

func Server(raw net.Conn, self ed25519.PrivateKey, v *members.View) (*Conn, error) {
	var peer members.Member
	cfg := config(self, v, nil, &peer)
	cfg.ClientAuth = tls.RequireAnyClientCert
	return open(tls.Server(raw, cfg), v, &peer)
}

func Client(raw net.Conn, self ed25519.PrivateKey, v *members.View, host ed25519.PublicKey) (*Conn, error) {
	var peer members.Member
	return open(tls.Client(raw, config(self, v, host, &peer)), v, &peer)
}

func config(self ed25519.PrivateKey, v *members.View, want ed25519.PublicKey, peer *members.Member) *tls.Config {
	return &tls.Config{
		MinVersion:         tls.VersionTLS13,
		Certificates:       []tls.Certificate{cert(self)},
		InsecureSkipVerify: true,
		VerifyPeerCertificate: func(raw [][]byte, _ [][]*x509.Certificate) error {
			if len(raw) != 1 {
				return members.ErrNotMember
			}
			c, err := x509.ParseCertificate(raw[0])
			if err != nil {
				return err
			}
			key, ok := c.PublicKey.(ed25519.PublicKey)
			if !ok {
				return members.ErrNotMember
			}
			if want != nil && !key.Equal(want) {
				return ErrWrongPeer
			}
			m, ok := v.State().Has(key)
			if !ok {
				return members.ErrNotMember
			}
			*peer = m
			return nil
		},
	}
}

func open(c *tls.Conn, v *members.View, peer *members.Member) (*Conn, error) {
	c.SetDeadline(time.Now().Add(10 * time.Second))
	err := c.Handshake()
	if err == nil {
		err = exchangeLogs(c, v)
	}
	if err != nil {
		c.Close()
		return nil, err
	}
	c.SetDeadline(time.Time{})
	key := peer.Key
	m, ok := v.State().Has(key)
	stop := v.Subscribe(func(s members.State) {
		if now, ok := s.Has(key); !ok || now.Kind != m.Kind {
			go c.Close()
		}
	})
	if !ok {
		stop()
		c.Close()
		return nil, members.ErrNotMember
	}
	return &Conn{Conn: c, Peer: m, stop: stop}, nil
}

// exchangeLogs is how a removal the cloud hid reaches a machine: inside TLS,
// the cloud can't drop it without dropping the link.
func exchangeLogs(c *tls.Conn, v *members.View) error {
	sent := make(chan error, 1)
	go func() { sent <- writeLog(c, v.Log()) }()
	theirs, err := readLog(c)
	if err == nil {
		err = <-sent
	}
	if err != nil {
		return err
	}
	if err = v.Accept(theirs); errors.Is(err, members.ErrFork) {
		return nil
	}
	return err
}

func writeLog(w io.Writer, l members.Log) error {
	b, err := json.Marshal(l)
	if err != nil {
		return err
	}
	var n [4]byte
	binary.BigEndian.PutUint32(n[:], uint32(len(b)))
	_, err = w.Write(append(n[:], b...))
	return err
}

func readLog(r io.Reader) (members.Log, error) {
	var n [4]byte
	if _, err := io.ReadFull(r, n[:]); err != nil {
		return nil, err
	}
	size := binary.BigEndian.Uint32(n[:])
	if size > 1<<20 {
		return nil, fmt.Errorf("log of %d bytes", size)
	}
	b := make([]byte, size)
	if _, err := io.ReadFull(r, b); err != nil {
		return nil, err
	}
	var l members.Log
	return l, json.Unmarshal(b, &l)
}

func cert(key ed25519.PrivateKey) tls.Certificate {
	tmpl := &x509.Certificate{
		SerialNumber: big.NewInt(1),
		NotBefore:    time.Unix(0, 0),
		NotAfter:     time.Date(9999, 1, 1, 0, 0, 0, 0, time.UTC),
	}
	der, err := x509.CreateCertificate(rand.Reader, tmpl, tmpl, key.Public(), key)
	if err != nil {
		panic(err)
	}
	return tls.Certificate{Certificate: [][]byte{der}, PrivateKey: key}
}
