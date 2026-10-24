import Wire

extension Sample {
    public static let branches = BranchList(
        current: "main",
        local: [
            Branch(name: "main", at: now / 1000 - 600), Branch(name: "calm-otter", at: now / 1000 - 3_600),
            Branch(name: "fix/login", at: now / 1000 - 86_400), Branch(name: "release/0.1", at: now / 1000 - 864_000),
        ],
        remote: [
            Branch(name: "origin/main", at: now / 1000 - 600), Branch(name: "origin/feature/push", at: now / 1000 - 7_200),
            Branch(name: "origin/fix/login", at: now / 1000 - 86_400),
        ])

    public static let accounts: [Account] = [
        Account(id: "acc-work", provider: "claude", name: "Work", plan: "max", email: "mingo@work.dev", signedIn: true, isDefault: true,
                fiveHour: Usage(pct: 42, resetsAt: now / 1000 + 5_400), week: Usage(pct: 18, resetsAt: now / 1000 + 300_000)),
        Account(id: "acc-home", provider: "claude", name: "mingo@home.dev", plan: "pro", email: "mingo@home.dev", signedIn: true,
                fiveHour: Usage(pct: 96, resetsAt: now / 1000 + 1_200)),
        Account(id: "acc-old", provider: "claude", name: "Not signed in", signedIn: false),
    ]

    public static let providers = AgentProviders(
        providers: [
            ProviderInfo(id: "claude", available: true, efforts: ["low", "medium", "high", "xhigh", "max"], plan: true,
                         chat: ChatCaps(interrupt: true, compact: true, sendWhileWorking: true, subAgents: true, sendNow: true)),
            ProviderInfo(id: "codex", available: true, efforts: ["low", "medium", "high"], plan: false,
                         chat: ChatCaps(interrupt: true, compact: true, sendWhileWorking: false, subAgents: false)),
        ],
        maxAccess: "full", phoneMaxAccess: "edits")

    /// `projects` projects of `worktrees` worktrees each (the first the project's own), and `agents` agents spread over them,
    /// a fifth of them in no project. For timing the home list at scale.
    public static func workspace(projects: Int, worktrees: Int, agents: Int) -> (projects: [Project], agents: [AgentSummary]) {
        let list = (0..<projects).map { p in
            let root = "/Users/mingo/dev/project-\(p)"
            return Project(path: root, name: "project-\(p)", worktrees: (0..<worktrees).map { w in
                w == 0
                    ? Worktree(name: "project-\(p)", path: root, branch: "main", isMain: true)
                    : Worktree(name: "tree-\(w)", path: "/Users/mingo/.worktrees/project-\(p)/tree-\(w)", branch: "feature/\(p)-\(w)", isMain: false)
            })
        }
        let statuses: [AgentStatus] = [.working, .idle, .done, .needsYou, .idle]
        let summaries = (0..<agents).map { i in
            let tree = list[i % projects].worktrees[(i / projects) % worktrees]
            let cwd = i % 5 == 4 ? "/tmp/scratch-\(i)" : tree.path + (i % 3 == 0 ? "/src/app" : "")
            return AgentSummary(
                id: "a-\(i)", terminalId: "t-\(i)", title: "Session \(i)", cwd: cwd, provider: "claude", status: statuses[i % statuses.count],
                attached: true, pinned: i % 17 == 0 ? true : nil, epoch: 1, maxSeq: 0, createdAt: now - i * 60_000, updatedAt: now - i * 30_000)
        }
        return (list, summaries)
    }
}
