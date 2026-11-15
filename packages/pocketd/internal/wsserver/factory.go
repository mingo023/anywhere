package wsserver

import (
	"errors"
	"slices"

	"pocketd/internal/factory"
	"pocketd/internal/github"
	"pocketd/internal/proto"
)

// Why the Factory can't be turned on for a project, as its settings page says it.
const (
	factoryNoGh      = "Install GitHub CLI (gh) to use the Factory"
	factorySignedOut = "Run gh auth login to use the Factory"
	factoryNoRemote  = "This project has no GitHub remote"
)

// factoryVerb serves the Factory's config verbs for a project on this Mac.
func (c *conn) factoryVerb(m proto.ClientMessage) error {
	if c.s.FactoryConfigs == nil || !slices.Contains(c.caps, proto.CapFactory) {
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryOff, "The Factory is off"))
		return nil
	}
	if err := c.usableFolder(m.Project); err != nil {
		c.send(factoryError(m.ID, err))
		return nil
	}
	if m.Config != nil && !m.Config.Valid() {
		c.send(factoryError(m.ID, factory.ErrInvalid))
		return nil
	}
	c.factoryNext(func() {
		if m.Type == "factory.config.save" {
			c.factorySave(m)
			return
		}
		c.send(proto.NewFactoryConfig(m.ID, m.Project, c.s.FactoryConfigs.Get(m.Project), c.factoryProblem(m.Project)))
	})
	return nil
}

// factoryMetrics answers with a project's numbers. Reading every Ticket's log takes time, so it answers off the read loop.
func (c *conn) factoryMetrics(m proto.ClientMessage) error {
	if c.s.Factory == nil || !slices.Contains(c.caps, proto.CapFactory) {
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryOff, "The Factory is off"))
		return nil
	}
	if err := c.usableFolder(m.Project); err != nil {
		c.send(factoryError(m.ID, err))
		return nil
	}
	go func() { c.send(proto.NewFactoryMetrics(m.ID, m.Project, c.s.Factory.Metrics(m.Project))) }()
	return nil
}

// factorySave keeps a project's settings, unless they turn the Factory on
// where it can't work. While the Factory is on and stays on gh isn't asked, so
// a gh that signed out later can't lock the settings.
func (c *conn) factorySave(m proto.ClientMessage) {
	configs, cfg := c.s.FactoryConfigs, *m.Config
	if store := c.profiles(); store != nil {
		for _, id := range []string{cfg.Lead, cfg.Workers, cfg.Review} {
			if _, ok := store.Get(id); !ok {
				c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownProfile, "No profile "+id))
				return
			}
		}
	}
	problem := ""
	if !cfg.Enabled || !configs.Get(m.Project).Enabled {
		problem = c.factoryProblem(m.Project)
	}
	if cfg.Enabled && problem != "" {
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryUnavailable, problem))
		return
	}
	if err := configs.Save(m.Project, cfg); err != nil {
		c.send(factoryError(m.ID, err))
		return
	}
	c.send(proto.NewFactoryConfig(m.ID, m.Project, configs.Get(m.Project), problem))
}

// factoryNext runs do off the read loop, after this conn's earlier
// factory.config verbs have answered: asking gh takes seconds, and a save
// that overtook the one before it would leave the older settings saved.
func (c *conn) factoryNext(do func()) {
	before, done := c.factoryTurn, make(chan struct{})
	c.factoryTurn = done
	go func() {
		defer close(done)
		if before != nil {
			<-before
		}
		do()
	}()
}

// factoryProblem is why the Factory can't be turned on for project, or "".
// A gh that fails for any other reason, as it does offline, is no problem.
func (c *conn) factoryProblem(project string) string {
	if c.s.GitHub == nil {
		return factoryNoGh
	}
	switch c.s.GitHub.Status(c.ctx, project).State {
	case string(github.Missing):
		return factoryNoGh
	case string(github.SignedOut):
		return factorySignedOut
	}
	if _, problem := c.s.GitHub.PR(c.ctx, project); problem == github.NoRemote {
		return factoryNoRemote
	}
	return ""
}

func factoryError(id string, err error) proto.Error {
	switch {
	case errors.Is(err, factory.ErrInvalid):
		return proto.NewErrorCode(id, proto.CodeInvalidConfig, err.Error())
	case errors.Is(err, errUnknownProject), errors.Is(err, errFolderUnusable):
		return proto.NewErrorCode(id, proto.CodeUnknownProject, err.Error())
	}
	return proto.NewError(id, err.Error())
}
