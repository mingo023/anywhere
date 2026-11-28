import Domain
import Foundation
import Observation

/// The app's top-level sections, each across every paired Mac: tabs on iPhone, sidebar rows on iPad.
public enum Tab: String, Codable, CaseIterable, Hashable, Sendable {
    case inbox
    case sessions
    case prs
    case machines

    /// Saved by versions with a Terminals tab or a Settings tab, which Sessions and Machines replace.
    public init(from decoder: any Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        switch raw {
        case "terminals": self = .sessions
        case "settings": self = .machines
        default: self = Tab(rawValue: raw) ?? .inbox
        }
    }

    /// Whether the tab bar shows over a stack: on lists, not over a screen with its own pinned action or composer
    /// (a chat, a pull request, changes, a diff, history, files, a terminal).
    public static func showsBar(over path: [Route]) -> Bool {
        switch path.last {
        case nil, .sessions, .project, .worktree, .local, .automations, .pullRequests, .settings, .pairing, .newSession: true
        case .chat, .subAgent, .pullRequest, .reviewThreads, .changes, .diff, .history, .commit, .branches, .files, .file,
             .terminals, .terminal: false
        }
    }
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

/// Where the app is: the section and a stack of `Route`s per section, so deep links and state restoration can set it.
/// Routes name their Mac, so a stack can hold screens of several. The selected Mac is the one the iPad's trees and a
/// new session start from. A link to a Mac that isn't connected yet is held until it is, or until the user goes
/// somewhere else.
@MainActor
@Observable
public final class Navigator {
    public var tab = Tab.inbox {
        didSet { if tab != oldValue { held = nil } }
    }
    /// The selected Mac's device id.
    public private(set) var macId: String?
    public var inboxPath: [Route] = []
    public var sessionsPath: [Route] = []
    public var prsPath: [Route] = []
    public var machinesPath: [Route] = []
    /// On iPad, the project, worktree or Local (a `.project`, `.worktree` or `.local` route) the sessions list is narrowed to.
    public var tree: Route? {
        didSet { if tree != oldValue { held = nil } }
    }
    public var pairing: PairingRequest?
    /// A `.newSession` route shown as a sheet over the current screen.
    public var sheet: Route?
    /// A link that arrived before its Mac connected, opened once it does.
    public private(set) var held: DeepLink?
    /// What each place last showed on the iPad, as the desktop's worktrees each keep their own tabs.
    private var shownIn: [Route: [Route]] = [:]
    /// The Macs whose chats state restoration put back on a stack, not yet checked against the sessions they list.
    private var restoredChats: Set<String> = []

    public init() {}

    public func path(_ tab: Tab) -> [Route] {
        switch tab {
        case .inbox: inboxPath
        case .sessions: sessionsPath
        case .prs: prsPath
        case .machines: machinesPath
        }
    }

    /// The user moving through a stack drops a held link.
    public func setPath(_ path: [Route], for tab: Tab) {
        if path != self.path(tab) { held = nil }
        switch tab {
        case .inbox: inboxPath = path
        case .sessions: sessionsPath = path
        case .prs: prsPath = path
        case .machines: machinesPath = path
        }
    }

    /// Moves the iPad's sessions to `place`, and the chat with it unless one of its `chats` is already open: back to what
    /// it last showed if that chat is still in `chats`, else to the first of them. Nil, the Mac's own Sessions, leaves the
    /// chat open.
    public func select(place: Route?, chats: [Route]) {
        guard place != tree else { return }
        rememberPlace()
        tree = place
        guard let place, !sessionsPath.contains(where: chats.contains) else { return }
        let kept = shownIn[place] ?? []
        let live = kept.allSatisfy { if case .chat = $0 { chats.contains($0) } else { true } }
        setPath(live && !kept.isEmpty ? kept : Array(chats.prefix(1)), for: .sessions)
    }

    private func rememberPlace() {
        if let tree { shownIn[tree] = sessionsPath }
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
        for tab in Tab.allCases {
            let path = path(tab)
            if let at = path.firstIndex(where: { $0.tree == tree }) { setPath(Array(path[..<at]), for: tab) }
        }
    }

    /// Opens a link from inside the app, as a new session's chat or an automation run's: a chat on a known Mac is
    /// pushed over the current screen, so Back returns there and the iPad keeps its tree. Other links open as from outside.
    public func show(_ link: DeepLink, macs: [MacRef]) {
        guard case .agent(let mac, _) = link, tab != .machines, let chat = link.routes.last, macs.contains(where: { $0.named(mac) })
        else { return open(link, macs: macs) }
        if path(tab).last != chat { push(chat) }
    }

    private var stacks: [Route] { Tab.allCases.flatMap(path) }

    /// The agent ids of every chat or sub-agent of `mac` on a stack: the chats whose stores stay.
    public func chats(on mac: String) -> Set<String> {
        Set(stacks.compactMap { route in
            switch route {
            case .chat(mac, let agentId), .subAgent(mac, let agentId, _): agentId
            default: nil
            }
        })
    }

    /// The Macs with a chat or sub-agent on a stack, by the name routes give them.
    public var chatMacs: Set<String> {
        Set(stacks.compactMap { route in
            switch route {
            case .chat(let mac, _), .subAgent(let mac, _, _): mac
            default: nil
            }
        })
    }

    public func dropHeld() { held = nil }

    /// Selects a Mac, dropping a held link as any move does. The stacks stay, as each route names its Mac; the iPad stops
    /// narrowing to the last Mac's tree.
    public func select(mac id: String) {
        guard id != macId else { return }
        held = nil
        switchMac(to: id)
    }

    private func switchMac(to id: String) {
        rememberPlace()
        macId = id
        tree = nil
    }

    /// Keeps the selection on a Mac that exists: the first one when none is selected or the selected one was forgotten.
    public func keepSelection(_ macs: [MacRef]) {
        if let macId, macs.contains(where: { $0.id == macId }) { return }
        // Not the user moving, so a held link stays.
        if let first = macs.first {
            switchMac(to: first.id)
        } else {
            macId = nil
            tree = nil
        }
        dropForgotten(macs)
    }

    /// Leaves the screens of a Mac no longer paired, with what was opened over them.
    private func dropForgotten(_ macs: [MacRef]) {
        for tab in Tab.allCases {
            let path = path(tab)
            let gone = path.firstIndex { route in route.mac.map { mac in !macs.contains { $0.named(mac) } } ?? false }
            if let gone { setPath(Array(path[..<gone]), for: tab) }
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
            if tab != .inbox { tab = .sessions }
            select(mac: ref.id)
            tree = nil
            guard ref.online else {
                held = link
                return
            }
            held = nil
            // The tab's root already shows every Mac's sessions.
            setPath(Array(link.routes.dropFirst()), for: tab)
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
    /// What state restoration keeps. Older saves have `terminals` and `settings` stacks and no Inbox or PRs ones.
    public struct Saved: Codable, Hashable, Sendable {
        public var tab: Tab
        public var macId: String?
        public var inbox: [Route] = []
        public var sessions: [Route] = []
        public var prs: [Route] = []
        public var machines: [Route] = []
        public var tree: Route?

        init(tab: Tab, macId: String?, inbox: [Route], sessions: [Route], prs: [Route], machines: [Route], tree: Route?) {
            self.tab = tab
            self.macId = macId
            self.inbox = inbox
            self.sessions = sessions
            self.prs = prs
            self.machines = machines
            self.tree = tree
        }

        private enum CodingKeys: String, CodingKey {
            case tab, macId, inbox, sessions, prs, machines, tree, settings
        }

        public init(from decoder: any Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            tab = try c.decode(Tab.self, forKey: .tab)
            macId = try c.decodeIfPresent(String.self, forKey: .macId)
            inbox = try c.decodeIfPresent([Route].self, forKey: .inbox) ?? []
            sessions = try c.decodeIfPresent([Route].self, forKey: .sessions) ?? []
            prs = try c.decodeIfPresent([Route].self, forKey: .prs) ?? []
            machines = try c.decodeIfPresent([Route].self, forKey: .machines) ?? c.decodeIfPresent([Route].self, forKey: .settings) ?? []
            tree = try c.decodeIfPresent(Route.self, forKey: .tree)
        }

        public func encode(to encoder: any Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(tab, forKey: .tab)
            try c.encodeIfPresent(macId, forKey: .macId)
            try c.encode(inbox, forKey: .inbox)
            try c.encode(sessions, forKey: .sessions)
            try c.encode(prs, forKey: .prs)
            try c.encode(machines, forKey: .machines)
            try c.encodeIfPresent(tree, forKey: .tree)
        }
    }

    public var saved: Saved {
        Saved(tab: tab, macId: macId, inbox: inboxPath, sessions: sessionsPath, prs: prsPath, machines: machinesPath, tree: tree)
    }

    public func restore(_ saved: Saved, macs: [MacRef]) {
        tab = saved.tab
        inboxPath = saved.inbox
        sessionsPath = saved.sessions
        prsPath = saved.prs
        machinesPath = saved.machines
        if let id = saved.macId, macs.contains(where: { $0.id == id }) {
            macId = id
            tree = saved.tree
        }
        keepSelection(macs)
        restoredChats = chatMacs
    }

    /// Once a restored Mac lists its sessions, drops restored chats of its sessions it no longer has, with whatever was
    /// opened over them. Chats opened since are left to show they closed.
    public func dropClosedChats(mac: String, listed: Set<String>) {
        guard restoredChats.remove(mac) != nil else { return }
        for tab in Tab.allCases {
            let path = path(tab)
            let closed = path.firstIndex { route in
                if case .chat(mac, let agentId) = route { !listed.contains(agentId) } else { false }
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
