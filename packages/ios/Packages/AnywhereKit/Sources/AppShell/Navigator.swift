import Domain
import Foundation
import Observation

/// The app's top-level sections: tabs on iPhone, sidebar rows on iPad.
public enum Tab: String, Codable, CaseIterable, Hashable, Sendable {
    case sessions
    case terminals
    case settings
}

/// What navigation needs to know of a paired Mac.
public struct MacRef: Hashable, Sendable {
    /// The device id pairing gave; `HostModel.id`.
    public var id: String
    /// Its `hello.ok` serverId, once known: what routes and deep links name it by.
    public var serverId: String?
    public var online: Bool

    public init(id: String, serverId: String?, online: Bool) {
        self.id = id
        self.serverId = serverId
        self.online = online
    }

    func named(_ mac: String) -> Bool { serverId == mac || id == mac }
}

/// A pairing sheet to show, from a pair link or "Pair another Mac".
public struct PairingRequest: Identifiable, Hashable, Sendable {
    public let id = UUID()
    public var link: URL?
    public var knownHost: String?

    public init(link: URL? = nil, knownHost: String? = nil) {
        self.link = link
        self.knownHost = knownHost
    }
}

/// Where the app is: the section, the selected Mac, and a stack of `Route`s per section, so deep links and state
/// restoration can set it. A link to a Mac that isn't connected yet is held until it is.
@MainActor
@Observable
public final class Navigator {
    public var tab = Tab.sessions
    /// The selected Mac's device id.
    public private(set) var macId: String?
    public var sessionsPath: [Route] = []
    public var terminalsPath: [Route] = []
    public var settingsPath: [Route] = []
    public var pairing: PairingRequest?
    /// A link that arrived before its Mac connected, opened once it does.
    public private(set) var held: DeepLink?

    public init() {}

    public func path(_ tab: Tab) -> [Route] {
        switch tab {
        case .sessions: sessionsPath
        case .terminals: terminalsPath
        case .settings: settingsPath
        }
    }

    public func setPath(_ path: [Route], for tab: Tab) {
        switch tab {
        case .sessions: sessionsPath = path
        case .terminals: terminalsPath = path
        case .settings: settingsPath = path
        }
    }

    /// Pushes onto the current section's stack, as a chat's links do.
    public func push(_ route: Route) {
        setPath(path(tab) + [route], for: tab)
    }

    /// Selects a Mac. Routes belong to one Mac, so switching clears the Mac's stacks.
    public func select(mac id: String) {
        guard id != macId else { return }
        macId = id
        sessionsPath = []
        terminalsPath = []
    }

    /// Keeps the selection on a Mac that exists: the first one when none is selected or the selected one was forgotten.
    public func keepSelection(_ macs: [MacRef]) {
        if let macId, macs.contains(where: { $0.id == macId }) { return }
        if let first = macs.first {
            select(mac: first.id)
        } else {
            macId = nil
            sessionsPath = []
            terminalsPath = []
        }
    }

    /// Opens a link from the scheme, a notification or the Live Activity. A chat waits for its Mac to connect.
    public func open(_ link: DeepLink, macs: [MacRef]) {
        switch link {
        case .pair(let url):
            held = nil
            pairing = PairingRequest(link: url)
        case .mac(let mac):
            guard let ref = macs.first(where: { $0.named(mac) }) else {
                held = link
                return
            }
            held = nil
            tab = .sessions
            select(mac: ref.id)
            sessionsPath = []
        case .agent(let mac, _):
            guard let ref = macs.first(where: { $0.named(mac) }) else {
                held = link
                return
            }
            tab = .sessions
            select(mac: ref.id)
            guard ref.online else {
                held = link
                return
            }
            held = nil
            // The tab's root already shows the Mac's sessions.
            sessionsPath = Array(link.routes.dropFirst())
        }
    }

    /// Opens the held link once its Mac is known and, for a chat, connected.
    public func release(_ macs: [MacRef]) {
        guard let link = held else { return }
        switch link {
        case .mac(let mac) where macs.contains(where: { $0.named(mac) }): open(link, macs: macs)
        case .agent(let mac, _) where macs.contains(where: { $0.named(mac) && $0.online }): open(link, macs: macs)
        default: break
        }
    }

    /// What state restoration keeps.
    public struct Saved: Codable, Hashable, Sendable {
        public var tab: Tab
        public var macId: String?
        public var sessions: [Route]
        public var terminals: [Route]
        public var settings: [Route]
    }

    public var saved: Saved {
        Saved(tab: tab, macId: macId, sessions: sessionsPath, terminals: terminalsPath, settings: settingsPath)
    }

    public func restore(_ saved: Saved, macs: [MacRef]) {
        tab = saved.tab
        settingsPath = saved.settings
        guard let id = saved.macId, macs.contains(where: { $0.id == id }) else { return keepSelection(macs) }
        macId = id
        sessionsPath = saved.sessions
        terminalsPath = saved.terminals
    }

    public func encoded() -> Data? { try? JSONEncoder().encode(saved) }

    public func restore(_ data: Data, macs: [MacRef]) {
        guard let saved = try? JSONDecoder().decode(Saved.self, from: data) else { return keepSelection(macs) }
        restore(saved, macs: macs)
    }
}
