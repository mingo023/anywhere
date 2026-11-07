package daemon

import (
	"errors"
	"log"
	"sync"

	"pocketd/internal/agent"
)

var errNoMailbox = errors.New("pocketd has no mailbox")

// Mail keeps text for agentID's chat until a turn that showed it ends normally.
func (d *Daemon) Mail(agentID, text string) error {
	if d.Mailbox == nil {
		return errNoMailbox
	}
	if _, err := d.Mailbox.Send(agentID, text); err != nil {
		return err
	}
	d.Pump(agentID)
	return nil
}

// Pump delivers agentID's queued mail off the caller's goroutine: a status
// change calls it under the agent's lock.
func (d *Daemon) Pump(agentID string) {
	if d.Mailbox != nil {
		go d.deliver(agentID)
	}
}

func (d *Daemon) deliver(agentID string) {
	lock := d.mailLock(agentID)
	lock.Lock()
	defer lock.Unlock()
	a, err := d.Agents.Get(agentID)
	if err != nil {
		return
	}
	to, ok := a.Driver().(agent.Deliverer)
	if !ok || !takesMail(a.Summary().Status) {
		return
	}
	queued, err := d.Mailbox.Queued(agentID)
	if err != nil {
		log.Printf("mail %s: %v", agentID, err)
		return
	}
	for _, m := range queued {
		ref, err := to.Deliver(m.Text)
		if err != nil {
			log.Printf("mail %s: deliver: %v", agentID, err)
			return
		}
		if err := d.Mailbox.Shown(m.ID, ref); err != nil {
			log.Printf("mail %s: %v", agentID, err)
			return
		}
	}
}

// takesMail holds mail while the chat waits on the user.
func takesMail(status string) bool {
	return status == "idle" || status == "working" || status == "done"
}

func (d *Daemon) mailLock(agentID string) *sync.Mutex {
	l, _ := d.mailing.LoadOrStore(agentID, &sync.Mutex{})
	return l.(*sync.Mutex)
}

// settle acks the mail shown under ref, or puts it back in the queue; ref ""
// is all of agentID's shown mail.
func (d *Daemon) settle(agentID, ref string, ok bool) {
	if d.Mailbox == nil {
		return
	}
	lock := d.mailLock(agentID)
	lock.Lock()
	err := d.Mailbox.Settle(agentID, ref, ok)
	lock.Unlock()
	if err != nil {
		log.Printf("mail %s: settle: %v", agentID, err)
	}
}
