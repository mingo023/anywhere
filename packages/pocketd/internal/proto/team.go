package proto

// CodeTeamBusy refuses a team prompt while the lead's team is running, in review or merging.
const CodeTeamBusy = "team_busy"

// CapTeam: team prompts, team.list, team.update, team.start, team.stop and team.merge.
const CapTeam = "team.v1"

// Team is a lead's plan and how its tasks stand. pocketd keeps one per lead.
type Team struct {
	Lead      string     `json:"lead"`
	State     string     `json:"state"` // planning | proposed | running | review | merging | merged | conflict | failed
	Error     string     `json:"error,omitempty"`
	Tasks     []TeamTask `json:"tasks"`
	Ready     bool       `json:"ready,omitempty"`    // the lead said "Ready to merge"
	Conflict  []string   `json:"conflict,omitempty"` // files of the failed merge
	Repaired  bool       `json:"repaired,omitempty"` // the one repair turn was used
	Checkout  string     `json:"checkout,omitempty"` // the lead's checkout a merge runs in
	UpdatedAt int64      `json:"updatedAt"`
}

// TeamTask is one task of a Team, done by one worker.
type TeamTask struct {
	ID           string   `json:"id"`
	Title        string   `json:"title"`
	Agent        string   `json:"agent"` // claude | codex
	Model        string   `json:"model,omitempty"`
	Files        []string `json:"files"`
	After        []string `json:"after"`
	Allow        []string `json:"allow"`
	Instructions string   `json:"instructions"`
	State        string   `json:"state"`            // planned | waiting | running | needsYou | done | failed | stopped | merged
	Worker       string   `json:"worker,omitempty"` // worker's agent id
	Branch       string   `json:"branch,omitempty"`
	Worktree     string   `json:"worktree,omitempty"`
	Summary      string   `json:"summary,omitempty"` // worker's last text
	Stat         string   `json:"stat,omitempty"`    // git diff --stat against its base
	Turn         string   `json:"turn,omitempty"`    // the prompt id of the worker's turn last handed back
}

// TeamList answers team.list with every lead's team.
type TeamList struct {
	Type  string `json:"type"`
	ID    string `json:"id"`
	Teams []Team `json:"teams"`
}

func NewTeamList(id string, teams []Team) TeamList {
	out := make([]Team, len(teams))
	for i, t := range teams {
		out[i] = t.wire()
	}
	return TeamList{"team.list", id, out}
}

// TeamUpdate is a team after any change, sent to every client.
type TeamUpdate struct {
	Type string `json:"type"`
	Team Team   `json:"team"`
}

func NewTeamUpdate(t Team) TeamUpdate { return TeamUpdate{"team.update", t.wire()} }

// wire fills the lists the schema requires, which a team read back from JSON may hold as null.
func (t Team) wire() Team {
	tasks := make([]TeamTask, len(t.Tasks))
	for i, k := range t.Tasks {
		k.Files, k.After, k.Allow = orEmpty(k.Files), orEmpty(k.After), orEmpty(k.Allow)
		tasks[i] = k
	}
	t.Tasks = tasks
	return t
}
