package proto

import (
	"encoding/json"
	"errors"
	"slices"
)

const CapHosts = "hosts.v1"

const (
	CodeUnknownHost = "unknown_host"
	CodeHostExists  = "host_exists"
	CodeInvalidHost = "invalid_host"
	CodeHostBusy    = "host_busy"
	// CodeHostSSH carries the failure class in Detail.
	CodeHostSSH = "host_ssh"
)

const (
	LinkConnecting = "connecting"
	LinkOnline     = "online"
	LinkRetrying   = "retrying"
	LinkFailed     = "failed"
)

var HostFailures = []string{"auth", "unknown_key", "key_changed", "no_such_host", "refused", "timed_out", "dropped", "throttled", "not_running", "unpaired", "path_too_long", "other"}

// HostTarget is how ssh reaches a host: kind "alias" (Name, and Port and User over its config's) or "address" (Addr, Port,
// User, KeyFile).
type HostTarget struct {
	Kind    string `json:"kind"`
	Name    string `json:"name,omitempty"`
	Addr    string `json:"addr,omitempty"`
	Port    int    `json:"port,omitempty"`
	User    string `json:"user,omitempty"`
	KeyFile string `json:"keyFile,omitempty"`
}

// HostLinkState is a host's connection; Failure is set for retrying and failed, Version and Uptime for online.
type HostLinkState struct {
	State   string
	Version string
	Uptime  uint64
	Failure string
}

func (l HostLinkState) MarshalJSON() ([]byte, error) {
	switch l.State {
	case LinkConnecting:
		return json.Marshal(struct {
			State string `json:"state"`
		}{l.State})
	case LinkOnline:
		return json.Marshal(struct {
			State   string `json:"state"`
			Version string `json:"version"`
			Uptime  uint64 `json:"uptime"`
		}{l.State, l.Version, l.Uptime})
	case LinkRetrying, LinkFailed:
		if !slices.Contains(HostFailures, l.Failure) {
			return nil, errors.New("proto: unknown host failure " + l.Failure)
		}
		return json.Marshal(struct {
			State   string `json:"state"`
			Failure string `json:"failure"`
		}{l.State, l.Failure})
	}
	return nil, errors.New("proto: unknown host link state " + l.State)
}

type Host struct {
	ID      string     `json:"id"`
	Name    string     `json:"name"`
	Target  HostTarget `json:"target"`
	Windows bool       `json:"windows,omitempty"`
	// Socket is the local end of the tunnel to the host's pocketd.
	Socket string        `json:"socket"`
	Link   HostLinkState `json:"link"`
}

// HostLink is a timeline item: a card asking the user to add a host, with Hint to fill its form.
type HostLink struct {
	EnvironmentID string `json:"environmentId,omitempty"`
	Hint          string `json:"hint,omitempty"`
	Reason        string `json:"reason,omitempty"`
}

// InstallProgress is a step of a host.install: Step is set for kind step, Text for warn and error, Final for an error
// trying again can't fix; running, done and error end it.
type InstallProgress struct {
	Kind  string `json:"kind"`
	Step  string `json:"step,omitempty"`
	Text  string `json:"text,omitempty"`
	Final bool   `json:"final,omitempty"`
}

// HostList answers host.list, and without an id is the whole list pushed to an owner.
type HostList struct {
	Type  string `json:"type"`
	ID    string `json:"id,omitempty"`
	Hosts []Host `json:"hosts"`
}

func NewHostList(id string, hosts []Host) HostList {
	if hosts == nil {
		hosts = []Host{}
	}
	return HostList{"host.list", id, hosts}
}

// HostUpdate answers host.add and host.edit, and without an id is one host pushed after it changed.
type HostUpdate struct {
	Type string `json:"type"`
	ID   string `json:"id,omitempty"`
	Host Host   `json:"host"`
}

func NewHostUpdate(id string, h Host) HostUpdate { return HostUpdate{"host.update", id, h} }

type HostProgress struct {
	Type     string          `json:"type"`
	HostID   string          `json:"hostId"`
	Update   bool            `json:"update"`
	Progress InstallProgress `json:"progress"`
}

func NewHostProgress(hostID string, update bool, p InstallProgress) HostProgress {
	return HostProgress{"host.progress", hostID, update, p}
}

var hostVerbs = map[string]verbDecoder{
	"host.list": func(fields, *ClientMessage) bool { return true },
	"host.add": func(f fields, m *ClientMessage) bool {
		return decodeHostTarget(f["target"], &m.HostTarget) && f.optionalString("name", &m.Name)
	},
	"host.edit": func(f fields, m *ClientMessage) bool {
		return f.text("hostId", &m.HostID) && decodeHostTarget(f["target"], &m.HostTarget)
	},
	"host.remove": func(f fields, m *ClientMessage) bool {
		return f.text("hostId", &m.HostID) && f.optionalBool("uninstall", &m.Uninstall)
	},
	"host.retry": func(f fields, m *ClientMessage) bool { return f.text("hostId", &m.HostID) },
	"host.install": func(f fields, m *ClientMessage) bool {
		return f.text("hostId", &m.HostID) && f.optionalBool("update", &m.Update) && f.optionalString("archiveDir", &m.ArchiveDir)
	},
	"host.sign_in": func(f fields, m *ClientMessage) bool {
		return f.text("hostId", &m.HostID) && f.text("password", &m.Password)
	},
}

func HostVerbs() []string {
	verbs := make([]string, 0, len(hostVerbs))
	for verb := range hostVerbs {
		verbs = append(verbs, verb)
	}
	slices.Sort(verbs)
	return verbs
}

func hostVerb(f fields, m *ClientMessage) (handled, ok bool) {
	decode, handled := hostVerbs[m.Type]
	return handled, handled && decode(f, m)
}

// decodeHostTarget reads the shape only; hosts.Valid refuses what ssh could take for an option.
func decodeHostTarget(raw json.RawMessage, dst **HostTarget) bool {
	var head struct {
		Kind string `json:"kind"`
	}
	var t HostTarget
	if json.Unmarshal(raw, &head) != nil {
		return false
	}
	var ok bool
	switch head.Kind {
	case "alias":
		ok = strictInto(raw, &t, []string{"kind", "name"}, "port", "user")
	case "address":
		ok = strictInto(raw, &t, []string{"kind", "addr", "port"}, "user", "keyFile")
	}
	if ok {
		*dst = &t
	}
	return ok
}
