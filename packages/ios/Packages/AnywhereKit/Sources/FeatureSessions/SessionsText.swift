import Domain
import Foundation
import Stores

/// Which sessions a Mac lists above its projects, by the Sessions home's chips.
enum SessionsFilter: String, CaseIterable, Hashable {
    case all = "All"
    case working = "Working"
    case pinned = "Pinned"

    /// The sessions listed, in the order the Sessions menu picked.
    func sessions(_ cards: [Sidebar.Card], by sort: Sidebar.SessionSort) -> [String] {
        let kept = self == .pinned ? cards.filter(\.pinned) : cards.filter { $0.status == .working }
        return Sidebar.sorted(kept, by: sort).map(\.id)
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
