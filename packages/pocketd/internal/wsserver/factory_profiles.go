package wsserver

import (
	"context"
	"errors"
	"slices"

	"pocketd/internal/factory"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

// factoryReadLimit fits a 2 MB profile whose files travel as base64.
const factoryReadLimit = 4 << 20

// readLimit is the largest frame a connection may send.
func readLimit(owner bool, caps []string) int64 {
	if owner && slices.Contains(caps, proto.CapFactory) {
		return factoryReadLimit
	}
	return authedReadLimit
}

func (c *conn) profiles() *factory.Profiles {
	if c.s.Factory == nil {
		return nil
	}
	return c.s.Factory.Profiles
}

// profileFeed sends the profile list now and after every change, to an owner that asked for the Factory.
func (c *conn) profileFeed(caps []string) {
	store := c.profiles()
	if store == nil || !c.who.Has(peer.Own) || !slices.Contains(caps, proto.CapFactory) || c.stopProfiles != nil {
		return
	}
	ctx, cancel := context.WithCancel(c.ctx)
	c.stopProfiles = cancel
	go func() {
		for {
			changed := store.Changed()
			c.send(proto.NewFactoryProfiles("", store.List()))
			select {
			case <-ctx.Done():
				return
			case <-changed:
			}
		}
	}()
}

func (c *conn) profileVerb(m proto.ClientMessage) error {
	store := c.profiles()
	if store == nil || !slices.Contains(c.caps, proto.CapFactory) {
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryOff, "The Factory is off"))
		return nil
	}
	var err error
	switch m.Type {
	case "factory.profile.get":
		doc, ok := store.Get(m.Profile)
		if !ok {
			err = factory.ErrUnknownProfile
			break
		}
		c.send(proto.NewFactoryProfile(m.ID, doc))
		return nil
	case "factory.profile.save":
		_, err = store.Save(*m.ProfileDoc)
	case "factory.profile.delete":
		err = store.Delete(m.Profile, c.s.Factory.Uses(m.Profile))
	}
	if err != nil {
		c.send(profileError(m.ID, err))
		return nil
	}
	c.send(proto.NewFactoryProfiles(m.ID, store.List()))
	return nil
}

func profileError(id string, err error) proto.Error {
	switch {
	case errors.Is(err, factory.ErrInUse):
		return proto.NewErrorCode(id, proto.CodeProfileInUse, err.Error())
	case errors.Is(err, factory.ErrUnknownProfile):
		return proto.NewErrorCode(id, proto.CodeUnknownProfile, err.Error())
	case errors.Is(err, factory.ErrInvalidProfile), errors.Is(err, factory.ErrBuiltIn):
		return proto.NewErrorCode(id, proto.CodeInvalidProfile, err.Error())
	}
	return proto.NewError(id, err.Error())
}
