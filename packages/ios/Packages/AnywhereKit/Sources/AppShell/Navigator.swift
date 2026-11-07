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
/// restoration can set it. A link to a Mac that isn't connected yet is held until it is, or until the user goes
/// somewhere else.
@MainActor
@Observable
public final class Navigator {
    public var tab = Tab.sessions {
        didSet { if tab != oldValue { held = nil } }
    }
    /// The selected Mac's device id.
    public private(set) var macId: String?
    public var sessionsPath: [Route] = []
    public var terminalsPath: [Route] = []
    public var settingsPath: [Route] = []
    /// On iPad, the project, worktree or Local (a `.project`, `.worktree` or `.local` route) the sessions list is narrowed to.
    public var tree: Route? {
        didSet { if tree != oldValue { held = nil } }
    }
    public var pairing: PairingRequest?
    /// A `.newSession` route shown as a sheet over the current screen.
    public var sheet: Route?
    /// A link that arrived before its Mac connected, opened once it does.
    public private(set) var held: DeepLink?
    /// The stacks hold chats state restoration put back, not yet checked against the sessions their Mac lists.
    private var restoredChats = false

    public init() {}

    public func path(_ tab: Tab) -> [Route] {
        switch tab {
        case .sessions: sessionsPath
        case .terminals: terminalsPath
        case .settings: settingsPath
        }
    }

    /// The user moving through a stack drops a held link.
    public func setPath(_ path: [Route], for tab: Tab) {
        if path != self.path(tab) { held = nil }
        switch tab {
        case .sessions: sessionsPath = path
        case .terminals: terminalsPath = path
        case .settings: settingsPath = path
        }
    }

    /// Pushes onto the current section's stack, as a chat's links do; New Session opens as a sheet, and a chat already on
    /// the stack is gone back to.
    public func push(_ route: Route) {
        let path = path(tab)
        if case .newSession = route {
            sheet = route
        } else if case .chat = route, let at = path.lastIndex(of: route) {
            setPath(Array(path[...at]), for: tab)
        } else {
            setPath(path + [route], for: tab)
        }
    }

    /// Leaves the screens of a tree being deleted, with what was opened over them, and on iPad stops narrowing to it.
    public func leave(tree: String) {
        if self.tree?.tree == tree { self.tree = nil }
        for tab in [Tab.sessions, .terminals] {
            let path = path(tab)
            if let at = path.firstIndex(where: { $0.tree == tree }) { setPath(Array(path[..<at]), for: tab) }
        }
    }

    /// Opens a link from inside the app, as a new session's chat or an automation run's: a chat on the selected Mac is
    /// pushed over the current screen, so Back returns there and the iPad keeps its tree. Other links open as from outside.
    public func show(_ link: DeepLink, macs: [MacRef]) {
        guard case .agent(let mac, _) = link, tab != .settings, let chat = link.routes.last,
              macs.contains(where: { $0.named(mac) && $0.id == macId })
        else { return open(link, macs: macs) }
        if path(tab).last != chat { push(chat) }
    }

    /// The agent ids of every chat on a stack: the chats whose stores stay.
    public var chats: Set<String> {
        Set((sessionsPath + terminalsPath + settingsPath).compactMap { route in
            if case .chat(_, let agentId) = route { agentId } else { nil }
        })
    }

    public func dropHeld() { held = nil }

    /// Selects a Mac. Routes belong to one Mac, so switching clears the Mac's stacks, and drops a held link as any move does.
    public func select(mac id: String) {
        guard id != macId else { return }
        held = nil
        switchMac(to: id)
    }

    private func switchMac(to id: String) {
        macId = id
        sessionsPath = []
        terminalsPath = []
        tree = nil
        restoredChats = false
    }

    /// Keeps the selection on a Mac that exists: the first one when none is selected or the selected one was forgotten.
    public func keepSelection(_ macs: [MacRef]) {
        if let macId, macs.contains(where: { $0.id == macId }) { return }
        // Not the user moving, so a held link stays.
        if let first = macs.first {
            switchMac(to: first.id)
        } else {
            macId = nil
            sessionsPath = []
            terminalsPath = []
            tree = nil
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
            tree = nil
        case .agent(let mac, _):
            guard let ref = macs.first(where: { $0.named(mac) }) else {
                held = link
                return
            }
            tab = .sessions
            select(mac: ref.id)
            tree = nil
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
        public var tree: Route?
    }

    public var saved: Saved {
        Saved(tab: tab, macId: macId, sessions: sessionsPath, terminals: terminalsPath, settings: settingsPath, tree: tree)
    }

    public func restore(_ saved: Saved, macs: [MacRef]) {
        tab = saved.tab
        settingsPath = saved.settings
        guard let id = saved.macId, macs.contains(where: { $0.id == id }) else { return keepSelection(macs) }
        macId = id
        sessionsPath = saved.sessions
        terminalsPath = saved.terminals
        tree = saved.tree
        restoredChats = !chats.isEmpty
    }

    /// Once the restored Mac lists its sessions, drops restored chats of sessions it no longer has, with whatever was
    /// opened over them. Chats opened since are left to show they closed.
    public func dropClosedChats(listed: Set<String>) {
        guard restoredChats else { return }
        restoredChats = false
        for tab in [Tab.sessions, .terminals] {
            let path = path(tab)
            let closed = path.firstIndex { route in
                if case .chat(_, let agentId) = route { !listed.contains(agentId) } else { false }
            }
            if let closed { setPath(Array(path[..<closed]), for: tab) }
        }
    }

    public func encoded() -> Data? { try? JSONEncoder().encode(saved) }

    public func restore(_ data: Data, macs: [MacRef]) {
        guard let saved = try? JSONDecoder().decode(Saved.self, from: data) else { return keepSelection(macs) }
        restore(saved, macs: macs)
    }
}
