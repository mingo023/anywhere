// Package members keeps an account's member list as a chain of signed ops.
// The cloud stores and forwards the chain; only members can extend it.
package members

import (
	"bytes"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base32"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"slices"
	"strings"
	"sync"
)

type Kind string

const (
	Machine Kind = "machine"
	Desktop Kind = "desktop"
	Phone   Kind = "phone"
)

type Member struct {
	Key  ed25519.PublicKey `json:"key"`
	Kind Kind              `json:"kind"`
	Name string            `json:"name"`
}

// Op is one change to the list. Genesis is signed by the member it names;
// every later op by a member already in the list, or an add by the recovery key.
type Op struct {
	Type     OpType            `json:"type"`
	Prev     []byte            `json:"prev,omitempty"`
	Member   Member            `json:"member"`
	Recovery ed25519.PublicKey `json:"recovery,omitempty"`
	// RecoverySig proves the genesis came from whoever held the recovery code;
	// the public key alone can be copied into a genesis the cloud made up.
	RecoverySig []byte            `json:"recoverySig,omitempty"`
	By          ed25519.PublicKey `json:"by"`
	Sig         []byte            `json:"sig,omitempty"`
}

type OpType string

const (
	opGenesis OpType = "genesis"
	opAdd     OpType = "add"
	opRemove  OpType = "remove"
)

var (
	ErrBadSig       = errors.New("bad signature")
	ErrNotMember    = errors.New("signer is not a member")
	ErrScope        = errors.New("a phone can add only a phone")
	ErrBadChain     = errors.New("op does not follow the one before it")
	ErrOtherAccount = errors.New("log belongs to another account")
	ErrFork         = errors.New("log forks from ours")
	ErrNotAdded     = errors.New("log does not add this device")
	ErrCodeMismatch = errors.New("code does not match")
)

func (o Op) body() []byte {
	o.Sig, o.RecoverySig = nil, nil
	b, _ := json.Marshal(o)
	return b
}

func (o Op) hash() [32]byte {
	b, _ := json.Marshal(o)
	return sha256.Sum256(b)
}

func sign(by ed25519.PrivateKey, o Op) Op {
	o.By = by.Public().(ed25519.PublicKey)
	o.Sig = ed25519.Sign(by, o.body())
	return o
}

type Log []Op

func Genesis(first ed25519.PrivateKey, m Member, recovery ed25519.PrivateKey) Log {
	o := Op{Type: opGenesis, Member: m, By: first.Public().(ed25519.PublicKey)}
	if recovery != nil {
		o.Recovery = recovery.Public().(ed25519.PublicKey)
		o.RecoverySig = ed25519.Sign(recovery, o.body())
	}
	return Log{sign(first, o)}
}

func (l Log) Add(by ed25519.PrivateKey, m Member) Log {
	return l.append(by, Op{Type: opAdd, Member: m})
}

func (l Log) Remove(by ed25519.PrivateKey, key ed25519.PublicKey) Log {
	return l.append(by, Op{Type: opRemove, Member: Member{Key: key}})
}

func (l Log) append(by ed25519.PrivateKey, o Op) Log {
	h := l[len(l)-1].hash()
	o.Prev = h[:]
	return append(slices.Clip(l), sign(by, o))
}

type State struct {
	Root     [32]byte
	Recovery ed25519.PublicKey
	members  map[string]Member
}

func (s State) Has(key ed25519.PublicKey) (Member, bool) {
	m, ok := s.members[string(key)]
	return m, ok
}

func (s State) Len() int { return len(s.members) }

// Replay checks every op of l and returns the list it ends with.
func Replay(l Log) (State, error) {
	if len(l) == 0 || l[0].Type != opGenesis || !bytes.Equal(l[0].By, l[0].Member.Key) || l[0].Prev != nil {
		return State{}, ErrBadChain
	}
	s := State{Root: l[0].hash(), Recovery: l[0].Recovery, members: map[string]Member{}}
	if r := l[0].Recovery; r != nil && (len(r) != ed25519.PublicKeySize || !ed25519.Verify(r, l[0].body(), l[0].RecoverySig)) {
		return State{}, fmt.Errorf("op 0: recovery: %w", ErrBadSig)
	}
	for i, o := range l {
		if len(o.By) != ed25519.PublicKeySize || len(o.Member.Key) != ed25519.PublicKeySize || !ed25519.Verify(o.By, o.body(), o.Sig) {
			return State{}, fmt.Errorf("op %d: %w", i, ErrBadSig)
		}
		if i > 0 {
			prev := l[i-1].hash()
			if !bytes.Equal(o.Prev, prev[:]) {
				return State{}, fmt.Errorf("op %d: %w", i, ErrBadChain)
			}
			signer, member := s.members[string(o.By)]
			byRecovery := o.Type == opAdd && s.Recovery != nil && bytes.Equal(o.By, s.Recovery)
			if !member && !byRecovery {
				return State{}, fmt.Errorf("op %d: %w", i, ErrNotMember)
			}
			// A phone holds a phone's scopes; letting it add a desktop would raise them to an owner's.
			if member && signer.Kind == Phone && o.Type == opAdd && o.Member.Kind != Phone {
				return State{}, fmt.Errorf("op %d: %w", i, ErrScope)
			}
		}
		switch o.Type {
		case opGenesis:
			if i > 0 {
				return State{}, fmt.Errorf("op %d: %w", i, ErrBadChain)
			}
			s.members[string(o.Member.Key)] = o.Member
		case opAdd:
			s.members[string(o.Member.Key)] = o.Member
		case opRemove:
			delete(s.members, string(o.Member.Key))
		default:
			return State{}, fmt.Errorf("op %d: %w", i, ErrBadChain)
		}
	}
	return s, nil
}

type Nonce [32]byte

func NewNonce() Nonce {
	var n Nonce
	rand.Read(n[:])
	return n
}

// Commit is what a new device sends with its key, before it sees any root.
func Commit(n Nonce) [32]byte { return sha256.Sum256(append([]byte("zoes-join-commit:"), n[:]...)) }

// CheckReveal is the approver's check that the nonce a new device revealed
// is the one it committed to.
func CheckReveal(commit [32]byte, nd Nonce) error {
	if Commit(nd) != commit {
		return ErrCodeMismatch
	}
	return nil
}

// SAS is the 8-digit code the new device and its approver both show. It
// binds the new key to the account's root. Eight digits alone could be
// ground by a cloud that picks a fake root, so the new device commits to nd
// before it sees the root and the approver's na, and reveals nd after: the
// cloud must fix its fake root before it can compute the code, and matches
// one time in 10^8.
func SAS(root [32]byte, key ed25519.PublicKey, nd, na Nonce) string {
	b := append(root[:], key...)
	b = append(b, nd[:]...)
	h := sha256.Sum256(append(b, na[:]...))
	n := binary.BigEndian.Uint64(h[:8]) % 100_000_000
	return fmt.Sprintf("%04d-%04d", n/10_000, n%10_000)
}

// NewRecoveryCode returns 130 random bits. The cloud sees the public key the
// code derives, so a short code would fall to an offline search.
func NewRecoveryCode() string {
	b := make([]byte, 17)
	rand.Read(b)
	s := base32.StdEncoding.WithPadding(base32.NoPadding).EncodeToString(b)[:26]
	return s[:5] + "-" + s[5:10] + "-" + s[10:15] + "-" + s[15:20] + "-" + s[20:]
}

func RecoveryKey(code string) ed25519.PrivateKey {
	norm := strings.ToUpper(strings.ReplaceAll(code, "-", ""))
	seed := sha256.Sum256([]byte("zoes-recovery-v1:" + norm))
	return ed25519.NewKeyFromSeed(seed[:])
}

// View is one member's trusted copy of the list. It only grows by logs that
// extend it from the same root.
type View struct {
	mu    sync.Mutex
	log   Log
	state State
	subs  map[int]func(State)
	next  int
}

func NewView(l Log) (*View, error) {
	s, err := Replay(l)
	if err != nil {
		return nil, err
	}
	return &View{log: l, state: s, subs: map[int]func(State){}}, nil
}

// Join trusts a log the cloud handed a new device, once the user typed the
// code an existing member showed. na came with the log; nd is the device's own.
func Join(l Log, self ed25519.PublicKey, nd, na Nonce, typed string) (*View, error) {
	s, err := Replay(l)
	if err != nil {
		return nil, err
	}
	if _, ok := s.Has(self); !ok {
		return nil, ErrNotAdded
	}
	if typed != SAS(s.Root, self, nd, na) {
		return nil, ErrCodeMismatch
	}
	return NewView(l)
}

// JoinWithCode adds self with the recovery key. The code proves the root, so
// no member has to approve.
func JoinWithCode(l Log, self Member, code string) (Log, *View, error) {
	rk := RecoveryKey(code)
	s, err := Replay(l)
	if err != nil {
		return nil, nil, err
	}
	if !bytes.Equal(s.Recovery, rk.Public().(ed25519.PublicKey)) {
		return nil, nil, ErrCodeMismatch
	}
	l = l.Add(rk, self)
	v, err := NewView(l)
	return l, v, err
}

func (v *View) State() State {
	v.mu.Lock()
	defer v.mu.Unlock()
	return v.state
}

func (v *View) Log() Log {
	v.mu.Lock()
	defer v.mu.Unlock()
	return v.log
}

// Accept takes a log from the cloud or a peer. A shorter log that agrees
// with ours changes nothing, which is how the cloud can hide a removal.
func (v *View) Accept(l Log) error {
	s, err := Replay(l)
	if err != nil {
		return err
	}
	v.mu.Lock()
	if s.Root != v.state.Root {
		v.mu.Unlock()
		return ErrOtherAccount
	}
	for i := range min(len(l), len(v.log)) {
		if l[i].hash() != v.log[i].hash() {
			v.mu.Unlock()
			return ErrFork
		}
	}
	if len(l) <= len(v.log) {
		v.mu.Unlock()
		return nil
	}
	v.log, v.state = l, s
	subs := make([]func(State), 0, len(v.subs))
	for _, f := range v.subs {
		subs = append(subs, f)
	}
	v.mu.Unlock()
	for _, f := range subs {
		f(s)
	}
	return nil
}

// Subscribe calls f with each list Accept adopts until the returned stop. It
// returns the list as it stood when f joined, so no change falls between.
func (v *View) Subscribe(f func(State)) (now State, stop func()) {
	v.mu.Lock()
	defer v.mu.Unlock()
	id := v.next
	v.next++
	v.subs[id] = f
	return v.state, func() {
		v.mu.Lock()
		delete(v.subs, id)
		v.mu.Unlock()
	}
}
