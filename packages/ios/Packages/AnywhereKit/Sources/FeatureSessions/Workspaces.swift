import Domain
import Stores

/// Where a session runs, as Up next and Working name it: its project and its branch, or its Local.
struct SessionPlace: Hashable {
    let project: String
    let tree: String

    var line: String { "\(project) · \(tree)" }
}

/// What the Sessions home says about projects and sessions, over plain data.
enum Workspaces {
    /// "GM" for glow-me; a one-word name gives its first two letters.
    static func initials(_ name: String) -> String {
        let words = name.split { !$0.isLetter && !$0.isNumber }
        let letters = words.count > 1 ? words.prefix(2).compactMap(\.first).map(String.init).joined() : String(words.first?.prefix(2) ?? "")
        return letters.isEmpty ? "?" : letters.uppercased()
    }

    /// "1 needs you": a roll-up's count and the status those sessions have.
    static func label(_ status: Status, count: Int) -> String? {
        status.label.map { "\(count) \($0.lowercased())" }
    }

    /// "1 failed, 2 done": how many sessions have each status, for VoiceOver.
    static func breakdown(_ statuses: [Status]) -> String {
        Sidebar.breakdown(statuses).compactMap { label($0.status, count: $0.count) }.joined(separator: ", ")
    }

    /// The sessions working now, the last to move first.
    static func working(_ cards: [Sidebar.Card]) -> [String] {
        cards.filter { $0.status == .working }.sorted { $0.updatedAt != $1.updatedAt ? $0.updatedAt > $1.updatedAt : $0.id < $1.id }.map(\.id)
    }

    /// What a project's corner dot says: the most urgent of the sessions that want a look.
    static func alert(_ statuses: [Status]) -> Status? {
        [.needsYou, .failed, .done].first { statuses.contains($0) }
    }

    /// "2 worktrees · 1 Local".
    static func line(worktrees: Int, locals: Int) -> String {
        var parts: [String] = []
        if worktrees > 0 { parts.append("\(worktrees) worktree\(worktrees == 1 ? "" : "s")") }
        if locals > 0 { parts.append("\(locals) Local\(locals == 1 ? "" : "s")") }
        if parts.isEmpty { parts.append("No worktrees") }
        return parts.joined(separator: " · ")
    }
}
