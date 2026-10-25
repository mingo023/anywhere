import Domain
import Stores

/// What the Workspaces home says about projects and sessions, over plain data.
enum Workspaces {
    /// "GM" for glow-me; a one-word name gives its first two letters.
    static func initials(_ name: String) -> String {
        let words = name.split { !$0.isLetter && !$0.isNumber }
        let letters = words.count > 1 ? words.prefix(2).compactMap(\.first).map(String.init).joined() : String(words.first?.prefix(2) ?? "")
        return letters.isEmpty ? "?" : letters.uppercased()
    }

    /// "1 running": a roll-up's count and what those sessions are doing.
    static func label(_ status: Status, count: Int) -> String? {
        let doing: String
        switch status {
        case .working: doing = "running"
        case .needsYou: doing = "waiting"
        case .failed: doing = "failed"
        case .done: doing = "done"
        case .idle: return nil
        }
        return "\(count) \(doing)"
    }

    /// The sessions to look at now: those waiting on you, then those running, each newest first.
    static func active(_ cards: [Sidebar.Card]) -> [String] {
        func rank(_ s: Status) -> Int? { s == .needsYou ? 0 : s == .working ? 1 : nil }
        return cards.filter { rank($0.status) != nil }
            .sorted { a, b in rank(a.status)! != rank(b.status)! ? rank(a.status)! < rank(b.status)! : a.updatedAt > b.updatedAt }
            .map(\.id)
    }

    /// "2 workspaces · now"; without a session to date it, the count alone.
    static func line(workspaces: Int, latest: Int, now: Int) -> String {
        let count = "\(workspaces) workspace\(workspaces == 1 ? "" : "s")"
        let ago = TimeText.ago(ms: latest, now: now)
        return ago.isEmpty ? count : "\(count) · \(ago)"
    }
}
