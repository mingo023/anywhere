package proto

import "slices"

type Range struct {
	Min int `json:"min"`
	Max int `json:"max"`
}

// CapScopes: hello.ok carries the scopes the peer was granted.
const CapScopes = "scopes.v1"

// CapHost gates hello.ok.host and host.changed.
const CapHost = "host.v1"

const CapRegistry = "registry.v1"

const CapSummaryV2 = "summary.v2"

// CapSubagents gates timeline items carrying parentToolUseId.
const CapSubagents = "subagents.v1"

// CapSettings: agent.settings switches a running agent's model or effort.
const CapSettings = "settings.v1"

// CapCommands: agent.commands lists the slash commands an agent's chat takes.
const CapCommands = "commands.v1"

// CapAccess: agent.settings switches a chat agent's access, and its summary names it.
const CapAccess = "access.v1"

// ServerCaps are the optional features this pocketd speaks, named <area>.v<n>.
var ServerCaps = []string{"pair.v1", CapScopes, CapHost, CapRegistry, CapSummaryV2, CapLaunch, CapRestore, CapOpen, CapNames, CapAutomations, CapLocals, CapFork, CapAccounts, CapSubagents, CapSettings, CapCommands, CapAccess, CapGit}

// Versions is the client's protocol range; a hello without one speaks only protocolVersion.
func (m ClientMessage) Versions() Range {
	if m.Protocol != nil {
		return *m.Protocol
	}
	return Range{int(m.ProtocolVersion), int(m.ProtocolVersion)}
}

// Negotiate picks the highest version both sides speak and the caps both have.
// With no common version, code names the older side: "client_too_old" or "server_too_old".
func Negotiate(client Range, clientCaps, serverCaps []string) (version int, caps []string, code string) {
	switch {
	case client.Max < MinVersion:
		return 0, nil, CodeClientTooOld
	case client.Min > MaxVersion:
		return 0, nil, CodeServerTooOld
	}
	caps = []string{}
	for _, c := range clientCaps {
		if slices.Contains(serverCaps, c) && !slices.Contains(caps, c) {
			caps = append(caps, c)
		}
	}
	slices.Sort(caps)
	return min(client.Max, MaxVersion), caps, ""
}
