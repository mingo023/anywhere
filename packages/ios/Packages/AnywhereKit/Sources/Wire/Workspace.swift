public struct Worktree: Codable, Hashable, Sendable {
    public var name: String
    public var path: String
    /// Empty on a detached head.
    public var branch: String
    public var isMain: Bool

    public init(name: String, path: String, branch: String, isMain: Bool) {
        self.name = name
        self.path = path
        self.branch = branch
        self.isMain = isMain
    }
}

public struct Project: Codable, Hashable, Sendable, Identifiable {
    /// pocketd's id for the project; stable when two projects share a name.
    public var projectId: String?
    public var path: String
    public var name: String
    public var worktrees: [Worktree]

    public var id: String { path }

    private enum CodingKeys: String, CodingKey {
        case projectId = "id", path, name, worktrees
    }

    public init(path: String, name: String, worktrees: [Worktree], projectId: String? = nil) {
        self.projectId = projectId
        self.path = path
        self.name = name
        self.worktrees = worktrees
    }
}

/// Every project: the reply to `project.list`, and pushed whole to workspace.v1 peers on each change.
public struct ProjectList: Codable, Hashable, Sendable {
    public var projects: [Project]
    public init(projects: [Project]) { self.projects = projects }
}

/// What a desktop's pocketd keeps: its projects with their settings, the tracked worktrees and the shared settings. The settings stay open JSON, which only desktops read.
public struct RegistryList: Codable, Hashable, Sendable {
    public struct Entry: Codable, Hashable, Sendable {
        public var id: String
        public var path: String
        public var repo: JSONValue
        public init(id: String, path: String, repo: JSONValue) {
            self.id = id
            self.path = path
            self.repo = repo
        }
    }

    public var projects: [Entry]
    public var tracked: [String]
    public var settings: JSONValue
    public init(projects: [Entry], tracked: [String], settings: JSONValue) {
        self.projects = projects
        self.tracked = tracked
        self.settings = settings
    }
}

/// Every worktree's display name by path, sent whole on hello and on each change.
public struct WorktreeNames: Codable, Hashable, Sendable {
    public var names: [String: String]
    public init(names: [String: String]) { self.names = names }
}

public struct WorktreeCreated: Codable, Hashable, Sendable {
    public var requestId: String
    public var path: String
    public var branch: String
    public init(requestId: String, path: String, branch: String) {
        self.requestId = requestId
        self.path = path
        self.branch = branch
    }
}

/// What deleting a worktree would close and lose, for it and its subagents' worktrees.
public struct WorktreeInspect: Codable, Hashable, Sendable {
    public struct Tree: Codable, Hashable, Sendable {
        public var host: String?
        public var path: String
        public var branch: String
        public var dirty: Int
        public var terminals: Int
        public init(host: String? = nil, path: String, branch: String, dirty: Int, terminals: Int) {
            self.host = host
            self.path = path
            self.branch = branch
            self.dirty = dirty
            self.terminals = terminals
        }
    }
    public var trees: [Tree]
    /// Commits a delete loses unless the branch is kept.
    public var lost: Int
    /// Commits on detached subagent heads, lost either way.
    public var lostAnyway: Int
    public init(trees: [Tree], lost: Int, lostAnyway: Int) {
        self.trees = trees
        self.lost = lost
        self.lostAnyway = lostAnyway
    }
}

public struct Branch: Codable, Hashable, Sendable {
    public var name: String
    /// Unix seconds of its tip commit.
    public var at: Int
    public init(name: String, at: Int) {
        self.name = name
        self.at = at
    }
}

/// Newest first, at most 200 each.
public struct BranchList: Codable, Hashable, Sendable {
    public var current: String?
    public var local: [Branch]
    public var remote: [Branch]
    public init(current: String? = nil, local: [Branch], remote: [Branch]) {
        self.current = current
        self.local = local
        self.remote = remote
    }
}

public struct Usage: Codable, Hashable, Sendable {
    /// 0 to 100.
    public var pct: Double
    /// Unix seconds.
    public var resetsAt: Int?
    public init(pct: Double, resetsAt: Int? = nil) {
        self.pct = pct
        self.resetsAt = resetsAt
    }
}

public struct Account: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var provider: String
    public var name: String
    public var plan: String?
    public var email: String?
    public var signedIn: Bool
    public var isDefault: Bool?
    public var fiveHour: Usage?
    public var week: Usage?

    public init(
        id: String, provider: String, name: String, plan: String? = nil, email: String? = nil, signedIn: Bool, isDefault: Bool? = nil,
        fiveHour: Usage? = nil, week: Usage? = nil
    ) {
        self.id = id
        self.provider = provider
        self.name = name
        self.plan = plan
        self.email = email
        self.signedIn = signedIn
        self.isDefault = isDefault
        self.fiveHour = fiveHour
        self.week = week
    }
}

public struct AccountList: Codable, Hashable, Sendable {
    public var accounts: [Account]
    public init(accounts: [Account]) { self.accounts = accounts }
}

extension ClientMessage {
    public struct ProjectPath: Codable, Hashable, Sendable {
        public var path: String
        public init(path: String) { self.path = path }
    }

    public struct ProjectAdd: Codable, Hashable, Sendable {
        public var path: String
        public var repo: JSONValue?
        public init(path: String, repo: JSONValue? = nil) {
            self.path = path
            self.repo = repo
        }
    }

    public struct ProjectSet: Codable, Hashable, Sendable {
        public var path: String
        public var repo: JSONValue
        public init(path: String, repo: JSONValue) {
            self.path = path
            self.repo = repo
        }
    }

    public struct WorktreeTrack: Codable, Hashable, Sendable {
        public var path: String
        public var tracked: Bool
        public init(path: String, tracked: Bool) {
            self.path = path
            self.tracked = tracked
        }
    }

    public struct SettingsShared: Codable, Hashable, Sendable {
        public var settings: JSONValue
        public init(settings: JSONValue) { self.settings = settings }
    }

    public struct WorktreeRename: Codable, Hashable, Sendable {
        public var path: String
        public var title: String
        public init(path: String, title: String) {
            self.path = path
            self.title = title
        }
    }

    /// At most one of `base`, `branch` and `pr`.
    public struct WorktreeCreate: Codable, Hashable, Sendable {
        public var requestId: String
        public var project: String
        public var name: String
        public var base: String?
        public var branch: String?
        public var pr: Int?
        public var setup: Bool?

        public init(requestId: String, project: String, name: String, base: String? = nil, branch: String? = nil, pr: Int? = nil, setup: Bool? = nil) {
            self.requestId = requestId
            self.project = project
            self.name = name
            self.base = base
            self.branch = branch
            self.pr = pr
            self.setup = setup
        }
    }

    public struct WorktreeDelete: Codable, Hashable, Sendable {
        public var path: String
        public var deleteBranch: Bool?
        /// Deletes the branch even when commits on it are nowhere else (`unpushed_commits` otherwise).
        public var force: Bool?
        public var trash: Bool?
        public var teardownSecs: Int?
        /// False skips the teardown script, after a `teardown_failed` refusal.
        public var teardown: Bool?
        /// The branch the user confirmed deleting; it stays if the worktree has left it since.
        public var branch: String?
        public init(path: String, deleteBranch: Bool? = nil, force: Bool? = nil, trash: Bool? = nil, teardownSecs: Int? = nil, teardown: Bool? = nil, branch: String? = nil) {
            self.path = path
            self.deleteBranch = deleteBranch
            self.force = force
            self.trash = trash
            self.teardownSecs = teardownSecs
            self.teardown = teardown
            self.branch = branch
        }
    }

    /// A verb about one tree: `branch.list`, `gh.pr`, `git.push` without a lease, `git.pull`.
    public struct TreeRef: Codable, Hashable, Sendable {
        public var tree: String
        public init(tree: String) { self.tree = tree }
    }

    public struct AccountList: Codable, Hashable, Sendable {
        public var provider: String?
        public init(provider: String? = nil) { self.provider = provider }
    }
}
