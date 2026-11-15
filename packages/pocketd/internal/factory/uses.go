package factory

import (
	"slices"

	"pocketd/internal/proto"
)

// Role is one project's Factory config, under the project's name.
type Role struct {
	Project string
	Config  proto.FactoryConfig
}

// Users lists who would lose profile id if it went: a project's role, a task
// of a plan that waits at the gate, and a task that has yet to finish. label
// names a lead's Ticket.
func Users(id string, roles []Role, teams []proto.Team, label func(lead string) string) []string {
	var users []string
	for _, r := range roles {
		for _, role := range []struct{ name, profile string }{{"Lead", r.Config.Lead}, {"Workers", r.Config.Workers}, {"Reviewer", r.Config.Review}} {
			if role.profile == id {
				users = append(users, r.Project+" "+role.name+" role")
			}
		}
	}
	for _, tm := range teams {
		for _, task := range tm.Tasks {
			if task.Profile == id && (tm.State == "proposed" || slices.Contains([]string{"waiting", "running", "needsYou"}, task.State)) {
				users = append(users, label(tm.Lead)+" "+task.ID)
			}
		}
	}
	return users
}

// Uses is Users over every project's config and every team.
func (e *Engine) Uses(id string) []string {
	f := e.Teams.Projects.Load()
	var roles []Role
	for _, project := range f.Projects {
		roles = append(roles, Role{f.Name(project), e.Configs.Get(project)})
	}
	teams, _ := e.Teams.Store.List()
	return Users(id, roles, teams, e.label)
}
