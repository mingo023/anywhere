import DesignSystem
import Domain
import Foundation
import Stores
import Wire

/// What the Inbox shows across every Mac, over plain data: which session sits in which group, what its Mac's link does
/// to its row, the line under the title, and the strip. The view only draws it.
enum InboxLayout {
    struct Session: Hashable {
        var id: String
        var status: Status
        var updatedAt: Int
    }

    struct Mac: Hashable {
        var id: String
        var name: String
        var reachability: Reachability
        /// When the Mac went, in ms, once what it shows is stale.
        var asOf: Int?
        var sessions: [Session]
    }

    /// A session in a group, with what its Mac's link does to its row.
    struct Entry: Hashable {
        var mac: String
        var id: String
        var dim: Double
        var locked: Bool

        var key: String { "\(mac)/\(id)" }
    }

    struct Strip: Hashable {
        var kind: NetStrip.Kind
        var text: String
        var retry: Bool
        /// The Mac it speaks of.
        var mac: String?
    }

    struct Built: Hashable {
        var needsYou: [Entry] = []
        var failed: [Entry] = []
        var finished: [Entry] = []
        var working: [Entry] = []
        var strip: Strip?

        var isEmpty: Bool { needsYou.isEmpty && failed.isEmpty && finished.isEmpty && working.isEmpty }

        /// "2 need you · 1 failed · 1 finished"; the strip stands in for it while there is one.
        var subtitle: String? {
            strip == nil ? InboxLayout.subtitle(needsYou: needsYou.count, failed: failed.count, finished: finished.count) : nil
        }
    }

    /// Newest first in each group, as the design lists them. `back`: a Mac has just come back, which the strip says
    /// while nothing else needs saying.
    static func build(_ macs: [Mac], back: Bool = false) -> Built {
        var found: [(entry: Entry, status: Status, updatedAt: Int, order: Int)] = []
        for (order, mac) in macs.enumerated() {
            let link = Link(mac.reachability)
            for s in mac.sessions where s.status != .idle {
                found.append((Entry(mac: mac.id, id: s.id, dim: link.dim, locked: link.locked), s.status, s.updatedAt, order))
            }
        }
        found.sort { a, b in
            a.updatedAt != b.updatedAt ? a.updatedAt > b.updatedAt : a.order != b.order ? a.order < b.order : a.entry.id < b.entry.id
        }
        var built = Built()
        for f in found {
            switch f.status {
            case .needsYou: built.needsYou.append(f.entry)
            case .failed: built.failed.append(f.entry)
            case .done: built.finished.append(f.entry)
            case .working: built.working.append(f.entry)
            case .idle: break
            }
        }
        if let mac = macs.first(where: { Link($0.reachability).strip != nil }), let kind = Link(mac.reachability).strip {
            built.strip = strip(kind, mac: mac, answersOff: built.needsYou.contains { $0.mac == mac.id })
        } else if back {
            built.strip = Strip(kind: .back, text: "Back online", retry: false, mac: nil)
        }
        return built
    }

    static func subtitle(needsYou: Int, failed: Int, finished: Int) -> String? {
        let parts = [needsYou > 0 ? "\(needsYou) need\(needsYou == 1 ? "s" : "") you" : nil, failed > 0 ? "\(failed) failed" : nil,
                     finished > 0 ? "\(finished) finished" : nil].compactMap(\.self)
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// What a Mac's link does to the rows it holds: a Mac that is dialling or gone shows its last data dimmed, and
    /// answers nothing.
    struct Link: Hashable {
        var dim: Double
        var locked: Bool
        var strip: NetStrip.Kind?

        init(_ reachability: Reachability) {
            switch reachability {
            case .online, .connecting:
                (dim, locked, strip) = (1, false, nil)
            case .reconnecting:
                (dim, locked, strip) = (0.5, true, .reconnecting)
            case .unreachable:
                (dim, locked, strip) = (0.5, true, .failed)
            case .revoked, .updateMac, .updatePhone, .stopped:
                (dim, locked, strip) = (0.42, true, .offline)
            }
        }
    }

    private static func strip(_ kind: NetStrip.Kind, mac: Mac, answersOff: Bool) -> Strip {
        switch kind {
        case .reconnecting:
            Strip(kind: kind, text: NetStrip.reconnecting(mac: mac.name), retry: false, mac: mac.id)
        case .failed:
            Strip(kind: kind, text: NetStrip.failed(mac: mac.name, asOf: mac.asOf), retry: true, mac: mac.id)
        default:
            Strip(kind: kind, text: NetStrip.offline(mac: mac.name, asOf: mac.asOf) + (answersOff ? " · answers disabled" : ""), retry: false, mac: mac.id)
        }
    }
}

/// What an Inbox row says, over a session and the ask it waits on.
enum InboxText {
    /// What leads a row, and whether it is code: the command or path a plain ask wants, else the session's title.
    static func headline(_ s: AgentSummary, _ status: Status, ask: PermissionRequest?) -> (text: String, code: Bool) {
        guard status == .needsYou, let ask else { return (SessionText.title(s), false) }
        switch ask.detail {
        case .shell(let command, _): return (command, true)
        case .read(let path), .edit(let path, _), .write(let path, _): return (path, true)
        default: return (SessionText.ask(ask), false)
        }
    }

    struct Part: Hashable {
        var text: String
        var code: Bool
    }

    /// The parts of "anywhere · flaky-checkout · MacBook Pro" a session has; the branch is code.
    static func place(project: String?, branch: String?, mac: String?) -> [Part] {
        [(project, false), (branch, true), (mac, false)].compactMap { text, code in
            text.flatMap { $0.isEmpty ? nil : Part(text: $0, code: code) }
        }
    }

    /// "Approved · sent to MacBook Pro", or "Denied".
    static func answered(_ decision: Decision, mac: String) -> String {
        decision == .allow ? "Approved · sent to \(mac)" : "Denied"
    }
}
