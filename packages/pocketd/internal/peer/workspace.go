package peer

// A phone that speaks workspace.v1 manages Locals as it makes worktrees: the
// wsserver checks its local.* against these names, and everyone else's
// against Needs' own entries, owner only.
func init() {
	for _, v := range []string{"local.create", "local.rename", "local.delete"} {
		Needs["ws:"+v+".workspace"] = Spawn
	}
}
