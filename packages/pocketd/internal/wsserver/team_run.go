package wsserver

import "pocketd/internal/proto"

// teamRun serves the verbs that run a team, through Teams (nil answers "Teams are off").
func (c *conn) teamRun(m proto.ClientMessage) error {
	var err error
	switch m.Type {
	case "team.start":
		err = c.s.Teams.Start(m.Lead, m.Tasks)
	case "team.stop":
		err = c.s.Teams.Stop(m.Lead, m.Task)
	case "team.merge":
		err = c.s.Teams.Merge(m.Lead)
	}
	if err != nil {
		return err
	}
	c.send(proto.NewAck(m.ID))
	return nil
}
