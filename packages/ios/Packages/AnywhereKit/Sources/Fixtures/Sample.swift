import Foundation
import Wire

/// Realistic data for tests, the gallery and the bench. Deterministic: the same call gives the same values.
public enum Sample {
    /// 2026-10-24 09:00 UTC, in Unix milliseconds.
    public static let now = 1_792_832_400_000
    public static let mac = "mingos-mbp"

    public static let hello = HelloOk(
        serverId: mac, hostname: "Mingo's MacBook Pro", protocolVersion: 3, caps: Caps.phone, protocol: .phone,
        scopes: [.observe, .drive, .approve, .spawn], host: HostState(tailnet: true, keepingAwake: true, terminals: true), version: "0.1.13")

    public static let projects: [Project] = [
        Project(path: "/Users/mingo/dev/anywhere", name: "anywhere", worktrees: [
            Worktree(name: "anywhere", path: "/Users/mingo/dev/anywhere", branch: "main", isMain: true),
            Worktree(name: "calm-otter", path: "/Users/mingo/.worktrees/anywhere/calm-otter", branch: "calm-otter", isMain: false),
            Worktree(name: "fix-login", path: "/Users/mingo/.worktrees/anywhere/fix-login", branch: "fix/login", isMain: false),
        ]),
        Project(path: "/Users/mingo/dev/website", name: "website", worktrees: [
            Worktree(name: "website", path: "/Users/mingo/dev/website", branch: "main", isMain: true),
            Worktree(name: "dark-mode", path: "/Users/mingo/.worktrees/website/dark-mode", branch: "", isMain: false),
        ]),
        Project(path: "/Users/mingo/notes", name: "notes", worktrees: [
            Worktree(name: "notes", path: "/Users/mingo/notes", branch: "", isMain: true),
        ]),
    ]

    public static let locals = [Local(id: "review", project: "/Users/mingo/dev/anywhere", name: "Review")]

    public static let worktreeNames = ["/Users/mingo/.worktrees/anywhere/calm-otter": "Phone terminal"]

    /// One agent per status, plus the states a card shows besides its status.
    public static let agents: [AgentSummary] = [
        agent("a-needs", "Approve the migration", status: .needsYou, minutesAgo: 3, worktree: 1, activity: "Approve Bash: rm -rf build"),
        agent("a-needs-old", "Pick a colour for the badge", status: .needsYou, minutesAgo: 21, worktree: 0, activity: "Which colour?"),
        agent("a-failed", "Fix the flaky timeline test", status: .done, failed: true, minutesAgo: 9, worktree: 2),
        agent("a-done", "Write the changelog for 0.1.13", status: .done, minutesAgo: 14, worktree: 0),
        agent("a-working", "Port the commit graph lanes", status: .working, minutesAgo: 1, worktree: 1, model: "claude-opus-5-5"),
        agent("a-compacting", "Refactor the diff renderer", status: .working, minutesAgo: 5, worktree: 0, compacting: true),
        agent("a-goal", "Make every golden test pass", status: .working, minutesAgo: 30, worktree: 2,
              goal: Goal(objective: "every golden test passes", status: "active", tokensUsed: 16510, tokenBudget: 50000)),
        agent("a-idle", "Explain the pairing flow", status: .idle, minutesAgo: 120, worktree: 3, provider: "codex"),
        agent("a-detached", "zsh", status: .working, minutesAgo: 45, worktree: 4, attached: false),
        agent("a-chat", "Review PR #118", status: .idle, minutesAgo: 60, worktree: 0, chatOnly: true, pinned: true),
    ]

    public static let permission = PermissionRequest(
        requestId: "r-needs", agentId: "a-needs", toolName: "Bash", detail: .shell(command: "rm -rf build", description: "Remove the build folder"),
        options: [PermissionOption(id: "always", label: "Yes, and always allow rm in /Users/mingo/.worktrees/anywhere/calm-otter")],
        feedback: true)

    public static let question = PermissionRequest(
        requestId: "r-question", agentId: "a-needs-old", toolName: "AskUserQuestion",
        detail: .other(name: "AskUserQuestion", input: .object(["questions": .array([.object([
            "question": .string("Which colour?"), "options": .array([.string("Blue"), .string("Orange")]),
        ])])]), description: nil))

    public static let terminals: [Terminal] = [
        Terminal(id: "t-a-working", cwd: "/Users/mingo/.worktrees/anywhere/calm-otter", cols: 120, rows: 40, title: "claude", agentId: "a-working"),
        Terminal(id: "t-shell", cwd: "/Users/mingo/dev/anywhere", cols: 120, rows: 40, title: "zsh"),
        Terminal(id: "t-review", cwd: "/Users/mingo/dev/anywhere", cols: 100, rows: 30, title: "git log", localId: "review"),
        Terminal(id: "t-exited", cwd: "/Users/mingo/dev/website", cols: 80, rows: 24, title: "npm test", exited: true),
    ]

    public static let automations = Automations(
        automations: [
            Automation(id: "au-review", name: "Morning review", prompt: "Review new PRs", provider: .claude, folder: "/Users/mingo/dev/anywhere",
                       schedule: .days([1, 2, 3, 4, 5], time: "09:00"), enabled: true, nextRunAt: now + 86_400_000),
            Automation(id: "au-deps", name: "Bump dependencies", prompt: "Update outdated deps and run the tests", provider: .codex,
                       folder: "/Users/mingo/dev/website", schedule: .interval(everyMin: 720), enabled: false, newWorktree: true),
        ],
        runs: [
            Run(id: "run-2", automationId: "au-review", status: .running, trigger: "manual", startedAt: now - 300_000, agentId: "a-working"),
            Run(id: "run-1", automationId: "au-review", status: .succeeded, trigger: "schedule", summary: "Reviewed 3 PRs.",
                startedAt: now - 86_400_000, finishedAt: now - 86_100_000),
        ])

    /// What pocketd sends after hello.ok, in its order.
    public static var snapshot: [ServerMessage] {
        [
            .agentList(AgentList(agents: agents)), .automations(automations), .permissionRequest(PermissionAsk(request: permission)),
            .permissionRequest(PermissionAsk(request: question)), .worktreeNames(WorktreeNames(names: worktreeNames)),
            .localList(LocalList(locals: locals)), .projectList(ProjectList(projects: projects)),
        ]
    }

    static func agent(
        _ id: String, _ title: String, status: AgentStatus, failed: Bool? = nil, minutesAgo: Int, worktree: Int, model: String? = nil,
        provider: String = "claude", attached: Bool = true, compacting: Bool? = nil, activity: String? = nil, goal: Goal? = nil,
        chatOnly: Bool? = nil, pinned: Bool? = nil
    ) -> AgentSummary {
        let trees = projects.flatMap { p in p.worktrees.map { (p, $0) } }
        let (project, tree) = trees[worktree % trees.count]
        let updated = now - minutesAgo * 60_000
        return AgentSummary(
            id: id, terminalId: "t-\(id)", title: title, cwd: tree.path, provider: provider, model: model ?? (provider == "claude" ? "claude-sonnet-5" : "gpt-5.5"),
            effort: "high", access: "edits", status: status, failed: failed, attached: attached, compacting: compacting, pinned: pinned, epoch: 1,
            maxSeq: 0, providerSessionId: "s-\(id)", createdAt: updated - 600_000, updatedAt: updated, project: project.name, worktree: tree.name,
            mainWorktree: tree.isMain ? true : nil, branch: tree.branch.isEmpty ? nil : tree.branch, tokensUsed: 48_000, contextWindow: 1_000_000,
            origin: "desktop", activity: activity, chatOnly: chatOnly, goal: goal)
    }
}
