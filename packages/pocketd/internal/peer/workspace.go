package peer

// A phone that speaks workspace.v1 manages Locals and names worktrees as it
// makes them: the wsserver checks its local.* and worktree.rename against
// these names, and everyone else's against Needs' own entries, owner only.
func init() {
	for _, v := range []string{"local.create", "local.rename", "local.delete", "worktree.rename"} {
		Needs["ws:"+v+".workspace"] = Spawn
	}
}
