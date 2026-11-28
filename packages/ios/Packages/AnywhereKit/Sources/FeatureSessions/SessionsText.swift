import Domain
import Foundation
import Stores

/// Which sessions a Mac lists above its projects, by the Sessions home's chips.
enum SessionsFilter: String, CaseIterable, Hashable {
    case all = "All"
    case working = "Working"
    case pinned = "Pinned"

    /// The sessions listed, the last to move first.
    func sessions(_ cards: [Sidebar.Card]) -> [String] {
        let kept = self == .pinned ? cards.filter(\.pinned) : cards.filter { $0.status == .working }
        return kept.sorted { $0.updatedAt != $1.updatedAt ? $0.updatedAt > $1.updatedAt : $0.id < $1.id }.map(\.id)
    }

    /// Only All lists projects: the others narrow to sessions.
    var showsProjects: Bool { self == .all }
}

/// What the Sessions home says about each Mac.
enum SessionsText {
    /// "Connected over Tailscale", "Offline · last seen 10:42".
    static func connection(_ reachability: Reachability, asOf: Int?, timeZone: TimeZone = .current) -> String {
        switch reachability {
        case .online: "Connected over Tailscale"
        case .connecting, .reconnecting, .revoked, .updateMac, .updatePhone: reachability.label
        case .unreachable, .stopped: asOf.map { "Offline · last seen \(TimeText.clock(ms: $0, timeZone: timeZone))" } ?? "Offline"
        }
    }

    /// A Mac that can't be reached reads as offline in grey, not as a fault in red.
    static func dot(_ reachability: Reachability) -> LinkTone {
        reachability == .unreachable ? .off : reachability.tone
    }
}
