import DesignSystem
import Domain
import Stores
import Wire

/// What session rows, the new session sheet and the Macs list say.
enum SessionText {
    static func title(_ s: AgentSummary) -> String {
        s.title.isEmpty ? "New session" : s.title
    }

    /// The line under a session's title: what failed, else what it is doing, else its status, else its model. Working
    /// shows as the glyph's spin, so it isn't spelled out; the other statuses are, as their dots differ by colour alone.
    static func detail(_ s: AgentSummary, _ status: Status) -> String? {
        if s.compacting == true { return "Compacting…" }
        if status == .failed, let error = s.error, !error.isEmpty { return error }
        if let doing = s.activity, !doing.isEmpty { return doing }
        if status != .working, let label = status.label { return label }
        return s.model.map(modelName).flatMap { $0.isEmpty ? nil : $0 }
    }

    /// What leads a session's row in Up next: the ask it waits on, what failed, else its title.
    static func headline(_ s: AgentSummary, _ status: Status, requests: [PermissionRequest]) -> String {
        switch status {
        case .needsYou: requests.first.map(ask) ?? title(s)
        case .failed: s.error.flatMap { $0.isEmpty ? nil : $0 } ?? title(s)
        default: title(s)
        }
    }

    /// The word beside a finished session's place, as dots alone can't say which it is.
    static func outcome(_ status: Status) -> String? {
        switch status {
        case .failed: status.label
        case .done: "Finished"
        default: nil
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

    /// "Open · 3 of 4 checks": a worktree's pull request on its tile.
    static func pullRequestState(_ item: PrItem) -> String {
        let total = item.checks.passed + item.checks.failed + item.checks.pending
        let state = item.draft ? "Draft" : item.state == "MERGED" ? "Merged" : item.state == "CLOSED" ? "Closed" : "Open"
        return total == 0 ? state : "\(state) · \(item.checks.passed) of \(total) checks"
    }

    /// The line under a session that wants a look: the ask it waits on, what failed, else `detail`.
    static func look(_ s: AgentSummary, _ status: Status, requests: [PermissionRequest]) -> String? {
        switch status {
        case .needsYou: requests.first.map(ask) ?? s.activity.flatMap { $0.isEmpty ? nil : $0 } ?? status.label
        case .failed: [s.error, s.activity].compactMap(\.self).first { !$0.isEmpty } ?? status.label
        default: detail(s, status)
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
        case .task(let description, _, _): "Wants to start \(description)"
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

    /// The steps of a launch still to come after `last`, in pocketd's order.
    static func stepsLeft(after last: String?) -> [String] {
        let order = ["prepare", "verify", "fetch", "worktree", "copy", "setup", "agent"]
        guard let last, let at = order.firstIndex(of: last) else { return [] }
        return Array(order[(at + 1)...])
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
