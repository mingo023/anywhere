import DesignSystem
import Domain
import Stores
import Wire

/// What session rows, the new session sheet and the Macs list say.
enum SessionText {
    static func title(_ s: AgentSummary) -> String {
        s.title.isEmpty ? "New session" : s.title
    }

    /// The line under a session's title: the ask it waits on, what it is doing, how it failed or that it finished, or Idle.
    static func note(_ s: AgentSummary, _ status: Status, requests: [PermissionRequest]) -> String? {
        let said = [s.error, s.activity].compactMap(\.self).first { !$0.isEmpty }
        switch status {
        case .needsYou: return requests.first.map(ask) ?? said ?? status.label
        case .working: return s.compacting == true ? "Compacting…" : said
        case .failed: return ["Failed", said].compactMap(\.self).joined(separator: " · ")
        case .done: return "Finished"
        case .idle: return "Idle"
        }
    }

    /// An ask answered with a plain yes or no and short enough to read whole on its row, so the row can offer them;
    /// anything with a choice, or a long command nobody should approve from a list, opens its chat.
    static func answersInPlace(_ p: PermissionRequest) -> Bool {
        switch p.detail {
        case .shell, .read, .edit, .write, .search, .task: ask(p).count <= 200
        case .other, .unknown: false
        }
    }

    /// "Wants to run rm -rf build", or the question AskUserQuestion asks (`Pending::ask` in `crates/agents`).
    static func ask(_ p: PermissionRequest) -> String {
        switch p.detail {
        case .shell(let command, _): "Wants to run \(command)"
        case .read(let path): "Wants to read \(path)"
        case .edit(let path, _): "Wants to edit \(path)"
        case .write(let path, _): "Wants to write \(path)"
        case .search(let query, _): "Wants to search \(query)"
        case .task(let description, _, _, _): "Wants to start \(description)"
        case .other(let name, let input, _):
            question(input) ?? "Wants to use \(name.isEmpty ? p.toolName : name)"
        case .unknown: "Wants to use \(p.toolName)"
        }
    }

    private static func question(_ input: JSONValue) -> String? {
        guard case .object(let o) = input, case .array(let qs)? = o["questions"], case .object(let first)? = qs.first,
              case .string(let q)? = first["question"], !q.isEmpty
        else { return nil }
        return q
    }

    /// A launch's row: "Creating git worktree…", or "Starting Claude…" for the agent.
    static func creating(_ c: Creation) -> String {
        creating(step: c.current, waiting: c.waiting, provider: c.plan?.spec.provider.rawValue ?? "")
    }

    static func creating(step: String?, waiting: Bool, provider id: String) -> String {
        if waiting { return "Waiting for your Mac…" }
        guard let step else { return "Asking your Mac…" }
        if step == "agent" { return "Starting \(AgentName.label(id))…" }
        return Self.step(step) + "…"
    }

    /// "#42 · 1 failed · 2 passed": a pull request's number and its checks.
    static func pullRequest(_ item: PrItem) -> String {
        let checks = [(item.checks.failed, "failed"), (item.checks.pending, "pending"), (item.checks.passed, "passed")]
            .filter { $0.0 > 0 }.map { "\($0.0) \($0.1)" }
        return (["#\(item.number)"] + (item.draft ? ["Draft"] : []) + checks).joined(separator: " · ")
    }

    /// Every step of a launch that makes a worktree, in pocketd's order.
    static let launchSteps = ["prepare", "verify", "fetch", "worktree", "copy", "setup", "agent"]

    /// The steps of a launch still to come after `last`, in `order`.
    static func stepsLeft(after last: String?, order: [String] = launchSteps) -> [String] {
        guard let last, let at = order.firstIndex(of: last) else { return [] }
        return Array(order[(at + 1)...])
    }

    static func launchWhere(_ plan: LaunchPlan, project: String?) -> String {
        func branch(_ names: String?..., base: String?) -> String {
            let name = names.compactMap { $0?.isEmpty == false ? $0 : nil }.first ?? "New worktree"
            return base.flatMap { $0.isEmpty ? nil : "\(name) from \($0)" } ?? name
        }
        if let w = plan.worktree { return branch(w.branch, w.name, w.pr.map { "PR #\($0)" }, base: w.base) }
        switch plan.spec.checkout {
        case .new(let new): return branch(new.branch, new.name, base: new.base)
        case .worktree(let path, _): return path.split(separator: "/").last.map(String.init) ?? path
        case .local: return [project, "Local"].compactMap(\.self).joined(separator: " · ")
        }
    }

    /// A launch step as the desktop labels it (`Step::label` in `pocket/src/creating.rs`).
    static func step(_ step: String) -> String {
        switch step {
        case "prepare": "Preparing"
        case "verify": "Checking branch name"
        case "fetch": "Fetching latest changes"
        case "worktree": "Creating git worktree"
        case "copy": "Copying configuration"
        case "setup": "Setting up"
        case "agent": "Starting agent"
        default: step.prefix(1).uppercased() + step.dropFirst()
        }
    }

    /// "Work · 42%": an account and the fuller of its two usage windows.
    static func account(_ a: Account) -> String {
        guard let peak = [a.fiveHour?.pct, a.week?.pct].compactMap(\.self).max() else { return a.name }
        return "\(a.name) · \(Int(peak.rounded()))%"
    }

    static func tone(_ status: Status) -> Pill.Tone {
        switch status {
        case .needsYou: .ink
        case .failed: .failed
        case .done: .success
        case .working: .working
        case .idle: .neutral
        }
    }
}
