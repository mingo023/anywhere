package wsserver

import (
	"errors"
	"slices"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

// withRegistry subscribes c to registry.list when it speaks registry.v2.
func (c *conn) withRegistry(caps []string) <-chan []byte {
	if c.s.Registry == nil || !slices.Contains(caps, proto.CapRegistryV2) || c.stopRegistry != nil {
		return nil
	}
	var msgs <-chan []byte
	msgs, c.stopRegistry = c.s.Registry.Subscribe()
	return msgs
}

// registryVerb serves project.add, project.remove, project.set, worktree.track and settings.shared; without the
// cap they are as unknown as they were before it.
func (c *conn) registryVerb(m proto.ClientMessage) error {
	if c.s.Registry == nil || !slices.Contains(c.caps, proto.CapRegistryV2) {
		return proto.ErrMalformed
	}
	err := c.changeRegistry(m)
	switch {
	case err == nil:
		c.send(proto.NewAck(m.ID))
		return nil
	case errors.Is(err, registry.ErrUnknownProject):
		c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownProject, err.Error()))
	default:
		c.send(proto.NewError(m.ID, err.Error()))
	}
	// The client shows its change before asking; the list as it stands takes a refused one back.
	c.send(c.s.Registry.List())
	return nil
}

func (c *conn) changeRegistry(m proto.ClientMessage) error {
	switch m.Type {
	case "project.add":
		return c.s.Registry.AddProject(m.Path, m.Repo)
	case "project.remove":
		return c.s.Registry.RemoveProject(m.Path)
	case "project.set":
		return c.s.Registry.SetRepo(m.Path, *m.Repo)
	case "worktree.track":
		return c.s.Registry.Track(m.Path, m.Tracked)
	case "settings.shared":
		return c.s.Registry.SetSettings(*m.Settings)
	}
	return proto.ErrMalformed
}
