//go:build unix

package peer

// pidOwns: the socket is 0600, so a peer outside every Terminal is the owner.
const pidOwns = true
