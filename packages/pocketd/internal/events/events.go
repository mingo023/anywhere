// Package events appends what the PRD §9 metrics are computed from to
// events.jsonl. It records ids and outcomes only: no titles, prompts, argv,
// env, paths or conversation text.
package events

import (
	"encoding/json"
	"log"
	"path/filepath"
	"sync"
	"time"

	"pocketd/internal/logfile"
)

const Layout = "2006-01-02T15:04:05.000Z07:00"

// Event is one line. Kinds and their fields: start{version,pid,service},
// stop{reason}, tick, status{agent,provider,from,to}, seen{agent,principal},
// answer{agent,principal,decision}, prompt{agent,provider,origin,ack},
// create{origin,provider,ok,code}, restore{agent,ok,ms},
// push{agent,alert,acted}, refusal{surface,code}.
type Event struct {
	TS        string `json:"ts"`
	Kind      string `json:"kind"`
	Version   string `json:"version,omitempty"`
	PID       int    `json:"pid,omitempty"`
	Service   *bool  `json:"service,omitempty"`
	Reason    string `json:"reason,omitempty"`
	Agent     string `json:"agent,omitempty"`
	Provider  string `json:"provider,omitempty"`
	From      string `json:"from,omitempty"`
	To        string `json:"to,omitempty"`
	Principal string `json:"principal,omitempty"`
	Decision  string `json:"decision,omitempty"`
	Origin    string `json:"origin,omitempty"`
	Ack       string `json:"ack,omitempty"`
	OK        *bool  `json:"ok,omitempty"`
	Code      string `json:"code,omitempty"`
	MS        int64  `json:"ms,omitempty"`
	Alert     string `json:"alert,omitempty"`
	Acted     *bool  `json:"acted,omitempty"`
	Surface   string `json:"surface,omitempty"`
}

type Log struct {
	f    *logfile.File
	once sync.Once
}

func Open(home string) (*Log, error) {
	f, err := logfile.Open(filepath.Join(home, "events.jsonl"), 10<<20, 2)
	if err != nil {
		return nil, err
	}
	return &Log{f: f}, nil
}

// Emit stamps e and appends it. A nil Log drops it; a failed write is logged once.
func (l *Log) Emit(e Event) {
	if l == nil {
		return
	}
	if e.TS == "" {
		e.TS = time.Now().UTC().Format(Layout)
	}
	raw, _ := json.Marshal(e)
	if _, err := l.f.Write(append(raw, '\n')); err != nil {
		l.once.Do(func() { log.Printf("events: %v", err) })
	}
}

func (l *Log) Close() error {
	if l == nil {
		return nil
	}
	return l.f.Close()
}
