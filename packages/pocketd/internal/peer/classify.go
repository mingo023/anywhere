package peer

import "pocketd/internal/proc"

// Ancestors lists pid and its parents, nearest first, stopping before launchd.
func Ancestors(pid int) ([]proc.Proc, error) {
	var chain []proc.Proc
	for pid > 1 {
		p, err := proc.Read(pid)
		if err != nil {
			return chain, err
		}
		chain = append(chain, p)
		next, err := proc.Parent(pid)
		if err != nil || next == pid {
			return chain, err
		}
		pid = next
	}
	return chain, nil
}

// Classify makes pid a PTY peer of the nearest Terminal root among its
// ancestors (itself included), else the owner. roots maps a live Terminal's
// root pid to its ID, for this pocketd only, so a scratch pocketd started
// inside a Pocket Terminal still has an owner. A walk that breaks off, as when
// a process on the way exits, fails closed to a PTY peer of no Terminal.
func Classify(pid int, roots map[int]string) Principal {
	for p := pid; p > 1; {
		if id, ok := roots[p]; ok {
			return Principal{Kind: PTY, Pid: pid, Terminal: id, Scopes: PTYScopes}
		}
		next, err := proc.Parent(p)
		if err != nil || next == p {
			return Principal{Kind: PTY, Pid: pid, Scopes: PTYScopes}
		}
		p = next
	}
	if !pidOwns {
		return Principal{Kind: PTY, Pid: pid, Scopes: PTYScopes}
	}
	return OwnerOf(pid)
}

// MayAuth is whether who can become the owner by sending an owner token.
func MayAuth(who Principal) bool { return mayAuth(who, pidOwns) }

// mayAuth: a TCP peer, or, where a pid can't prove the owner (Windows), a
// socket peer outside every Terminal; a token proves no more there than it
// would sent to ownerListen.
func mayAuth(who Principal, pidOwns bool) bool {
	return who.Kind == None || !pidOwns && who.Kind == PTY && who.Terminal == ""
}
