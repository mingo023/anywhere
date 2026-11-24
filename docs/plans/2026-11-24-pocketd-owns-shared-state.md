# pocketd owns shared state

## Why

`desktop.json` has three writers and two readers that disagree:

- The desktop reads it once at launch and rewrites it whole on every save.
- A remote desktop merges its projects into a host's `desktop.json` over SSH (`hosts/src/remote.rs` `sync_registry`).
- pocketd re-reads it on change and serves the project list to the phone.

So the phone (via pocketd) can list a project the host's desktop doesn't, and the host's desktop wipes it on its next save. Seen on 2026-11-24: a second `employer-ai-e2e` clone listed on iOS, not on the desktop, gone from the file after a desktop restart.

## Decisions

1. pocketd is the only writer of shared state: the project list, each project's settings, tracked worktrees, and the shared settings it reads (`git.remote`, `git.commit_all`, `git.force_with_lease`, `agents.access`, `worktree.root`, `sidebar.external_worktrees`). Clients change it only by messages; pocketd sends the whole state on hello and on each change.
2. Each project gets an id pocketd mints. Two clones of one repo are two projects.
3. pocketd imports once from `desktop.json` when its state file is missing (precedent: `hosts.ImportDesktop`). It never writes `desktop.json`.
4. The desktop keeps writing these fields to `desktop.json` as a cache of what pocketd sent: it shows them before pocketd answers, and an older pocketd (or a host not yet updated) still finds them. It never treats the file as the source once pocketd has sent a list.
5. Sidebar project order stays per app: the desktop's `projects` order; iOS its own.
6. Hard cut for the desktop: pocketd refuses a hello with `clientId: "desktop"` that lacks cap `registry.v2` (`client_too_old`). iOS also speaks protocol 3, so `MinVersion` is not bumped; iOS keeps working and ignores the new fields.
7. A session records its project id and tree when pocketd first places it; later placements change only the branch. Clients place a session by `projectId` and fall back to its folder when the id is missing or unknown.
8. The desktop no longer writes a host's `desktop.json`. It sends the host's pocketd messages. A host without `registry.v2` keeps its cached projects and the desktop says the host needs an update when the user changes one.
9. `worktree.clone_root`, `delete_branch`, `trash`, `teardown_secs`, `git.untracked/ai/agent/model`, and everything else in `desktop.json` stay desktop-only.

## Wire contract (cap `registry.v2`)

Server → client, after `hello.ok` to a client with the cap, and to all such clients on every change:

```json
{"type":"registry.list",
 "projects":[{"id":"p1a2b3c4d5e6f7a8","path":"/Users/me/app",
   "repo":{"name":"","color":0,"setup":"","teardown":"","launch":{"provider":"claude","model":"opus","effort":"","access":"full","account":""},
           "dev_url":"","icon":null,"gh_account":"","accounts":{},"hidden":[]}}],
 "tracked":["/Users/me/.anywhere/worktrees/app/cute-boa"],
 "settings":{"git":{"remote":"","commit_all":null,"force_with_lease":false},"access":"","worktree_root":"","external_worktrees":false}}
```

`repo` keys are the desktop's `RepoConfig` serialization. pocketd reads `name`, `setup`, `gh_account`, `accounts`; it keeps `launch` and `icon` as raw JSON and the rest typed, and round-trips all of them.

Client → server, owner only, needs `registry.v2`. Each answers `ack`, or an error and then the current `registry.list` so an optimistic client rolls back:

| Message | Fields | Rule |
|---|---|---|
| `project.add` | `path`, optional `repo` | Idempotent on `path` (an existing path acks unchanged). `path` must be an absolute existing folder, else error. |
| `project.remove` | `path` | Idempotent. Keeps nothing. |
| `project.set` | `path`, `repo` | Replaces the repo settings; the id stays. Unknown path → `unknown_project`. |
| `worktree.track` | `path`, `tracked` (bool) | Idempotent. |
| `settings.shared` | `settings` (the object above) | Replaces it whole. |

Additions to existing frames:
- `project.list`: each project gains `"id"`.
- Agent summary: `"projectId"` and `"tree"` (absolute path of the worktree, or the project folder for a non-repo project), beside the existing `project`/`worktree`/`mainWorktree`/`branch`.

## State file

`<home>/state/registry.json`, written with `atomicfile` and taken only after the write succeeds (as `locals.save`):

```json
{"projects":[{"id":"…","path":"…","repo":{…}}],"tracked":["…"],"settings":{…}}
```

Import when it's missing: from `<home>/desktop.json`, take `projects` not starting with `//` (those are other hosts'), their `repos`, `tracked`, `git.remote`, `git.commit_all`, `git.force_with_lease`, `agents.access`, `worktree.root`, `sidebar.external_worktrees`. Mint an id each. A missing `desktop.json` imports empty.

## Work

### pocketd (`packages/pocketd`)
- `internal/registry`: the `Registry` owns `state/registry.json` (open, import, mutate, publish via `hub`). `Load()` keeps returning `File`, so its ~22 readers don't change. `File.Repos[path].ID` carries the id. Fix the package comment.
- `cmd/pocketd/serve.go` and `cmd/pocketd/worktree.go`: open the registry from home.
- `internal/proto`: `CapRegistryV2`, the messages above, golden fixtures for each.
- `internal/wsserver`: subscribe clients with the cap to `registry.list`; handle the five messages (owner only, `peer` rules); refuse a desktop hello without the cap.
- `internal/worktree`: `Place` gains `ProjectID` and `Tree`; `internal/agent` `SetLocation` keeps the first non-empty project id and tree.
- `internal/workspace`: `project.list` rows carry `id`.

### desktop (`packages/desktop`)
- `agents`: cap `registry.v2`, `Event::Registry`, outbox senders for the five messages, `projectId`/`tree` on the summary.
- `hosts`: tag a host's `registry.list` paths with its prefix; drop `sync_registry`, `merged`, and `create`'s registry argument.
- `store`: `apply_registry(host, list)` replaces that machine's slice of `projects` (keeping the desktop's order, appending new ones), its `repos` and `tracked`, its ids, and for this Mac the shared settings. A host is pushed its cached `//host/…` projects once (`project.add` with `repo`) the first time it sends a list; remembered in `desktop.json`.
- `pocket`: every place that changes a shared field sends the message too; drop `sync_host_registry`/`host_registry`; place a local session by `projectId` first.
- `handoff`: when a host already has a project with the same origin, point at it instead of `git clone ~/code/<repo>`.

### iOS (`packages/ios`)
- `Project` decodes `id` as `projectId` (its `Identifiable.id` stays the path). Agent summary decodes `projectId` and `tree`.
- `HomeList.build` places a session by `projectId` (its `tree` when the project lists it, else the project's main tree), falling back to the folder `Index`.

## Done when

- A fresh pocketd home with a `desktop.json` imports its projects (not `//host` ones), repos, tracked and shared settings, each project with a distinct id; two clones named alike get two ids.
- Restarting pocketd keeps the same ids; editing `desktop.json` afterwards changes nothing pocketd serves.
- `project.add/remove/set`, `worktree.track`, `settings.shared` change what every subscribed client receives, and survive a restart.
- A desktop hello without `registry.v2` is refused; an iOS hello (protocol 3, no cap) still works.
- An agent started in a project carries its `projectId` and `tree`.
- The desktop shows the project list pocketd sends, keeps its own order, and its changes reach pocketd; it never writes a host's `desktop.json`.
- iOS and the desktop place a session in the same project when both have the id.
