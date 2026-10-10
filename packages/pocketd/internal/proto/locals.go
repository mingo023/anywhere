package proto

// CapLocals gates local.create, local.rename, local.delete, local.list and
// agent.create's {local} checkout.
const CapLocals = "locals.v1"

const CodeUnknownLocal = "unknown_local"

// Local is a named place over its project's own checkout. Its id never
// starts with "/", so it can't collide with a Worktree's path.
type Local struct {
	ID      string `json:"id"`
	Project string `json:"project"`
	Name    string `json:"name"`
}

// LocalList is every Local in creation order, sent whole on hello and on each change.
type LocalList struct {
	Type   string  `json:"type"`
	Locals []Local `json:"locals"`
}

func NewLocalList(locals []Local) LocalList {
	if locals == nil {
		locals = []Local{}
	}
	return LocalList{"local.list", locals}
}

// ValidLocalID is 1 to 64 of A-Z, a-z, 0-9, _ and -.
func ValidLocalID(id string) bool {
	if len(id) < 1 || len(id) > 64 {
		return false
	}
	for _, r := range id {
		if !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '_' || r == '-') {
			return false
		}
	}
	return true
}
