package wsserver

import (
	"context"
	"errors"
	"os"
	"slices"

	"pocketd/internal/automation"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

// automations keeps the owner's desktop current: a snapshot on hello and after every change.
// A client without the cap, like an older pocketd, gets Malformed message.
func (c *conn) automations(caps []string) {
	if c.s.Automations == nil || !c.who.Has(peer.Own) || !slices.Contains(caps, proto.CapAutomations) {
		return
	}
	if c.stopAutomations != nil {
		c.pushAutomations()
		return
	}
	ctx, cancel := context.WithCancel(c.ctx)
	c.stopAutomations = cancel
	go c.feedAutomations(ctx)
}

func (c *conn) feedAutomations(ctx context.Context) {
	for {
		changed := c.s.Automations.Changed()
		c.pushAutomations()
		select {
		case <-ctx.Done():
			return
		case <-changed:
		}
	}
}

// pushAutomations waits for a verb in flight, so its ack comes first.
func (c *conn) pushAutomations() {
	c.automationMu.Lock()
	defer c.automationMu.Unlock()
	c.send(proto.NewAutomations(c.s.Automations.Snapshot()))
}

func (c *conn) automation(m proto.ClientMessage) error {
	if c.s.Automations == nil || !slices.Contains(c.caps, proto.CapAutomations) {
		return proto.ErrMalformed
	}
	c.automationMu.Lock()
	defer c.automationMu.Unlock()
	if err := c.automate(m); err != nil {
		c.send(automationError(m.ID, err))
		return nil
	}
	c.send(proto.NewAck(m.ID))
	return nil
}

func (c *conn) automate(m proto.ClientMessage) error {
	store, now := c.s.Automations, c.s.Scheduler.Now()
	switch m.Type {
	case "automation.save":
		if err := c.usableFolder(m.Automation.Folder); err != nil {
			return err
		}
		_, err := store.Save(*m.Automation, now)
		return err
	case "automation.enable":
		return store.Enable(m.AutomationID, m.Enabled, now)
	case "automation.delete":
		return store.Delete(m.AutomationID)
	}
	_, err := c.s.Scheduler.RunNow(m.AutomationID)
	return err
}

var (
	errUnknownProject = errors.New("This folder isn't a project on your Mac")
	errFolderUnusable = errors.New("This folder can't be opened")
)

func (c *conn) usableFolder(folder string) error {
	if !slices.ContainsFunc(c.s.Projects(), func(p proto.Project) bool { return p.Path == folder }) {
		return errUnknownProject
	}
	if st, err := os.Stat(folder); err != nil || !st.IsDir() {
		return errFolderUnusable
	}
	return nil
}

func automationError(id string, err error) proto.Error {
	switch {
	case errors.Is(err, automation.ErrInvalid), errors.Is(err, errFolderUnusable):
		return proto.NewCodedError(id, proto.CodeInvalidAutomation, err.Error(), "")
	case errors.Is(err, automation.ErrUnknown):
		return proto.NewCodedError(id, proto.CodeUnknownAutomation, err.Error(), "")
	case errors.Is(err, errUnknownProject):
		return proto.NewCodedError(id, proto.CodeUnknownProject, err.Error(), "")
	case errors.Is(err, automation.ErrBusy):
		return proto.NewCodedError(id, proto.CodeAutomationBusy, err.Error(), "")
	}
	return proto.NewError(id, err.Error())
}
