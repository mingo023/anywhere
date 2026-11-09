import Domain
import Foundation
import Stores
import Testing
@testable import AppShell

private let online = MacRef(id: "d1", serverId: "mbp", online: true)
private let offline = MacRef(id: "d2", serverId: "studio", online: false)

@Suite("Navigator")
@MainActor
struct NavigatorTests {
    @Test("an agent link on a connected Mac selects it and opens the chat over its sessions")
    func agentLink() {
        let nav = Navigator()
        nav.tab = .settings
        nav.open(.agent(mac: "mbp", agentId: "a1"), macs: [online, offline])
        #expect(nav.tab == .sessions)
        #expect(nav.macId == "d1")
        #expect(nav.sessionsPath == [.chat(mac: "mbp", agentId: "a1")])
        #expect(nav.held == nil)
    }

    @Test("an agent link waits for its Mac to connect, showing the Mac meanwhile")
    func holdsUntilConnected() {
        let nav = Navigator()
        nav.open(.agent(mac: "studio", agentId: "a2"), macs: [online, offline])
        #expect(nav.macId == "d2")
        #expect(nav.sessionsPath.isEmpty)
        #expect(nav.held == .agent(mac: "studio", agentId: "a2"))
        nav.release([online, offline])
        #expect(nav.held != nil)
        nav.release([online, MacRef(id: "d2", serverId: "studio", online: true)])
        #expect(nav.sessionsPath == [.chat(mac: "studio", agentId: "a2")])
        #expect(nav.held == nil)
    }

    @Test("a link to a Mac not known yet is held until it is, then opened")
    func unknownMac() {
        let nav = Navigator()
        nav.open(.mac("later"), macs: [online])
        #expect(nav.held == .mac("later"))
        nav.release([online, MacRef(id: "d9", serverId: "later", online: false)])
        #expect(nav.macId == "d9")
        #expect(nav.held == nil)
    }

    @Test("a held link is dropped once the user goes somewhere else, or dismisses it")
    func heldDropped() {
        let nav = Navigator()
        let link = DeepLink.agent(mac: "studio", agentId: "a2")
        nav.open(link, macs: [online, offline])
        nav.setPath([], for: .sessions)
        #expect(nav.held == link, "a stack set to where it already is isn't a move")
        nav.push(.project(mac: "studio", path: "/p"))
        #expect(nav.held == nil)
        nav.open(link, macs: [online, offline])
        nav.tab = .terminals
        #expect(nav.held == nil)
        nav.open(link, macs: [online, offline])
        nav.tree = .worktree(mac: "studio", path: "/p/wt")
        #expect(nav.held == nil)
        nav.open(link, macs: [online, offline])
        nav.select(mac: "d1")
        #expect(nav.held == nil)
        nav.open(link, macs: [online, offline])
        nav.dropHeld()
        #expect(nav.held == nil)
    }

    @Test("a held link stays when the selection falls to another Mac on its own")
    func heldKeptOnFallback() {
        let nav = Navigator()
        nav.open(.mac("later"), macs: [])
        nav.keepSelection([online])
        #expect(nav.macId == "d1")
        #expect(nav.held == .mac("later"))
    }

    @Test("a chat opened from inside the app is pushed over the current screen and keeps the tree")
    func showPushes() {
        let nav = Navigator()
        nav.select(mac: "d1")
        nav.tree = .worktree(mac: "mbp", path: "/p/wt")
        nav.sessionsPath = [.worktree(mac: "mbp", path: "/p/wt")]
        nav.show(.agent(mac: "mbp", agentId: "a1"), macs: [online, offline])
        #expect(nav.sessionsPath == [.worktree(mac: "mbp", path: "/p/wt"), .chat(mac: "mbp", agentId: "a1")])
        #expect(nav.tree == .worktree(mac: "mbp", path: "/p/wt"))
        nav.show(.agent(mac: "mbp", agentId: "a1"), macs: [online, offline])
        #expect(nav.sessionsPath.count == 2, "the chat already on top isn't pushed twice")
    }

    @Test("a chat on another Mac opened from inside the app opens as a link from outside does")
    func showOtherMac() {
        let nav = Navigator()
        nav.select(mac: "d1")
        nav.sessionsPath = [.project(mac: "mbp", path: "/p")]
        nav.show(.agent(mac: "studio", agentId: "a2"), macs: [online, MacRef(id: "d2", serverId: "studio", online: true)])
        #expect(nav.macId == "d2")
        #expect(nav.sessionsPath == [.chat(mac: "studio", agentId: "a2")])
    }

    @Test("New Session opens as a sheet over the current screen, not on the stack")
    func newSessionSheet() {
        let nav = Navigator()
        nav.sessionsPath = [.chat(mac: "mbp", agentId: "a1")]
        nav.push(.newSession(mac: "mbp", project: "/p", place: "/p/wt", prompt: "Fix it"))
        #expect(nav.sheet == .newSession(mac: "mbp", project: "/p", place: "/p/wt", prompt: "Fix it"))
        #expect(nav.sessionsPath == [.chat(mac: "mbp", agentId: "a1")])
    }

    @Test("opening a chat already on the stack goes back to it rather than pushing it again")
    func pushGoesBack() {
        let nav = Navigator()
        let chat = Route.chat(mac: "mbp", agentId: "a1")
        nav.sessionsPath = [chat, .files(mac: "mbp", tree: "/p", dir: "", chat: "a1"), .file(mac: "mbp", tree: "/p", path: "a.rs", chat: "a1")]
        nav.push(chat)
        #expect(nav.sessionsPath == [chat])
        nav.push(.chat(mac: "mbp", agentId: "a2"))
        #expect(nav.sessionsPath == [chat, .chat(mac: "mbp", agentId: "a2")])
    }

    @Test("leaving a tree being deleted goes back past its screens and what was over them, and stops narrowing to it")
    func leaveTree() {
        let nav = Navigator()
        let tree = Route.worktree(mac: "mbp", path: "/p/wt")
        nav.sessionsPath = [.project(mac: "mbp", path: "/p"), tree, .pullRequest(mac: "mbp", tree: "/p/wt"), .chat(mac: "mbp", agentId: "a1")]
        nav.terminalsPath = [.terminal(mac: "mbp", terminalId: "t1")]
        nav.tree = tree
        nav.leave(tree: "/p/wt")
        #expect(nav.sessionsPath == [.project(mac: "mbp", path: "/p")])
        #expect(nav.terminalsPath == [.terminal(mac: "mbp", terminalId: "t1")])
        #expect(nav.tree == nil)

        nav.tree = .local(mac: "mbp", id: "l1")
        nav.sessionsPath = [.pullRequest(mac: "mbp", tree: "/p/wt")]
        nav.leave(tree: "/p/wt")
        #expect(nav.tree == .local(mac: "mbp", id: "l1"))
        #expect(nav.sessionsPath.isEmpty)
    }

    @Test("Show in chat from a sub-agent nested over its chat goes back to the chat")
    func showInChatGoesBack() {
        let nav = Navigator()
        let chat = Route.chat(mac: "mbp", agentId: "a1")
        nav.sessionsPath = [chat, .subAgent(mac: "mbp", agentId: "a1", callId: "c1"), .subAgent(mac: "mbp", agentId: "a1", callId: "c2")]
        nav.push(chat)
        #expect(nav.sessionsPath == [chat])
    }

    @Test("the chats on any stack are the ones whose stores stay")
    func chats() {
        let nav = Navigator()
        nav.sessionsPath = [.project(mac: "mbp", path: "/p"), .chat(mac: "mbp", agentId: "a1")]
        nav.terminalsPath = [.chat(mac: "mbp", agentId: "a2")]
        #expect(nav.chats == ["a1", "a2"])
    }

    @Test("a sub-agent restored on top of a stack keeps its chat's store")
    func subAgentKeepsItsChat() {
        let nav = Navigator()
        nav.sessionsPath = [.subAgent(mac: "mbp", agentId: "a1", callId: "c1")]
        #expect(nav.chats == ["a1"])
    }

    @Test("a restored chat of a session the Mac no longer lists is dropped with what was over it, once")
    func restoredChats() throws {
        let nav = Navigator()
        nav.select(mac: "d1")
        nav.sessionsPath = [.project(mac: "mbp", path: "/p"), .chat(mac: "mbp", agentId: "gone"), .file(mac: "mbp", tree: "/p", path: "a.rs")]
        nav.terminalsPath = [.chat(mac: "mbp", agentId: "kept")]
        let back = Navigator()
        back.restore(try #require(nav.encoded()), macs: [online])
        back.dropClosedChats(listed: ["kept"])
        #expect(back.sessionsPath == [.project(mac: "mbp", path: "/p")])
        #expect(back.terminalsPath == [.chat(mac: "mbp", agentId: "kept")])
        back.sessionsPath = [.chat(mac: "mbp", agentId: "new")]
        back.dropClosedChats(listed: [])
        #expect(back.sessionsPath == [.chat(mac: "mbp", agentId: "new")], "only the restored place is checked")
    }

    @Test("a Mac is found by its device id before it has said hello")
    func byDeviceId() {
        let nav = Navigator()
        nav.open(.mac("d2"), macs: [online, MacRef(id: "d2", serverId: nil, online: false)])
        #expect(nav.macId == "d2")
    }

    @Test("a pair link asks for the pairing sheet")
    func pairLink() {
        let nav = Navigator()
        let url = URL(string: "anywhere://pair?v=1&h=1.2.3.4:4517&c=abcdefghijklmnopqrstuv")!
        nav.open(.pair(url), macs: [])
        #expect(nav.pairing?.link == url)
    }

    @Test("switching Macs clears their stacks but keeps Settings'")
    func switching() {
        let nav = Navigator()
        nav.select(mac: "d1")
        nav.sessionsPath = [.chat(mac: "mbp", agentId: "a1")]
        nav.terminalsPath = [.terminal(mac: "mbp", terminalId: "t1")]
        nav.settingsPath = [.settings]
        nav.select(mac: "d1")
        #expect(!nav.sessionsPath.isEmpty, "selecting the same Mac keeps its place")
        nav.select(mac: "d2")
        #expect(nav.sessionsPath.isEmpty && nav.terminalsPath.isEmpty)
        #expect(nav.settingsPath == [.settings])
    }

    @Test("switching Macs, or a link to a chat, widens the sessions list from the tree it was narrowed to")
    func tree() {
        let nav = Navigator()
        nav.select(mac: "d1")
        nav.tree = .worktree(mac: "mbp", path: "/p/wt")
        nav.select(mac: "d1")
        #expect(nav.tree == .worktree(mac: "mbp", path: "/p/wt"))
        nav.select(mac: "d2")
        #expect(nav.tree == nil)
        nav.tree = .local(mac: "studio", id: "l1")
        nav.open(.agent(mac: "studio", agentId: "a2"), macs: [online, offline])
        #expect(nav.tree == nil)
    }

    @Test("the selection falls to the first Mac when none is selected or the selected one is forgotten")
    func keepsSelection() {
        let nav = Navigator()
        nav.keepSelection([online, offline])
        #expect(nav.macId == "d1")
        nav.select(mac: "d2")
        nav.keepSelection([online])
        #expect(nav.macId == "d1")
        nav.keepSelection([])
        #expect(nav.macId == nil)
    }

    @Test("the place survives encoding for state restoration, unless its Mac is gone")
    func restoration() throws {
        let nav = Navigator()
        nav.select(mac: "d2")
        nav.tab = .settings
        nav.terminalsPath = [.terminal(mac: "studio", terminalId: "t1")]
        nav.tree = .worktree(mac: "studio", path: "/p/wt")
        let data = try #require(nav.encoded())

        let back = Navigator()
        back.restore(data, macs: [online, offline])
        #expect(back.saved == nav.saved)

        let gone = Navigator()
        gone.restore(data, macs: [online])
        #expect(gone.macId == "d1")
        #expect(gone.terminalsPath.isEmpty)
        #expect(gone.tree == nil)
        #expect(gone.tab == .settings)
    }

    @Test("picking a worktree opens its newest chat in place of the last one's")
    func aWorktreeOpensItsNewestChat() {
        let nav = Navigator()
        nav.select(place: .worktree(mac: "mbp", path: "/a"), chats: [.chat(mac: "mbp", agentId: "a1"), .chat(mac: "mbp", agentId: "a2")])
        #expect(nav.sessionsPath == [.chat(mac: "mbp", agentId: "a1")])
        nav.select(place: .worktree(mac: "mbp", path: "/b"), chats: [.chat(mac: "mbp", agentId: "b1")])
        #expect(nav.sessionsPath == [.chat(mac: "mbp", agentId: "b1")])
    }

    @Test("a worktree with no sessions clears the chat rather than showing another's")
    func anEmptyWorktreeClearsTheChat() {
        let nav = Navigator()
        nav.select(place: .worktree(mac: "mbp", path: "/a"), chats: [.chat(mac: "mbp", agentId: "a1")])
        nav.select(place: .worktree(mac: "mbp", path: "/b"), chats: [])
        #expect(nav.sessionsPath.isEmpty)
    }

    @Test("going back to a worktree shows what it last showed, unless that chat has closed")
    func aWorktreeKeepsWhatItShowed() {
        let (a, b) = (Route.worktree(mac: "mbp", path: "/a"), Route.worktree(mac: "mbp", path: "/b"))
        let (older, newest) = (Route.chat(mac: "mbp", agentId: "older"), Route.chat(mac: "mbp", agentId: "newest"))
        let nav = Navigator()
        nav.select(place: a, chats: [newest, older])
        nav.setPath([older], for: .sessions)
        nav.select(place: b, chats: [])
        nav.select(place: a, chats: [newest, older])
        #expect(nav.sessionsPath == [older])
        nav.select(place: b, chats: [])
        nav.select(place: a, chats: [newest])
        #expect(nav.sessionsPath == [newest])
    }

    @Test("a worktree's chat that is already open stays when the worktree is picked")
    func anOpenChatStaysWhenItsWorktreeIsPicked() {
        let (first, second) = (Route.chat(mac: "mbp", agentId: "a1"), Route.chat(mac: "mbp", agentId: "a2"))
        let nav = Navigator()
        nav.setPath([second], for: .sessions)
        nav.select(place: .worktree(mac: "mbp", path: "/a"), chats: [first, second])
        #expect(nav.sessionsPath == [second])
    }

    @Test("a worktree keeps what it showed across a switch to another Mac and back")
    func aWorktreeKeepsItsChatAcrossMacs() {
        let a = Route.worktree(mac: "mbp", path: "/a")
        let (older, newest) = (Route.chat(mac: "mbp", agentId: "older"), Route.chat(mac: "mbp", agentId: "newest"))
        let nav = Navigator()
        nav.select(mac: "d1")
        nav.select(place: a, chats: [newest, older])
        nav.setPath([older], for: .sessions)
        nav.select(mac: "d2")
        nav.select(mac: "d1")
        nav.select(place: a, chats: [newest, older])
        #expect(nav.sessionsPath == [older])
    }

    @Test("picking the Mac's own Sessions leaves the chat open")
    func sessionsLeavesTheChatOpen() {
        let nav = Navigator()
        nav.select(place: .worktree(mac: "mbp", path: "/a"), chats: [.chat(mac: "mbp", agentId: "a1")])
        nav.select(place: nil, chats: [])
        #expect(nav.tree == nil)
        #expect(nav.sessionsPath == [.chat(mac: "mbp", agentId: "a1")])
    }

    @Test("a saved Terminals tab opens on Sessions, as the tab is gone")
    func savedTerminalsTabOpensOnSessions() throws {
        let nav = Navigator()
        nav.tab = .terminals
        let back = Navigator()
        back.restore(try #require(nav.encoded()), macs: [online])
        #expect(back.tab == .sessions)
    }
}

@Suite("Held link row")
struct HeldLinkRowTests {
    @Test("a held chat link names its session and its Mac, or just the Mac before the session is known")
    func text() {
        #expect(HeldLinkRow.text(session: "Fix the build", mac: "Studio") == "Opening Fix the build when Studio connects…")
        #expect(HeldLinkRow.text(session: nil, mac: "Studio") == "Opening the session when Studio connects…")
    }
}

@Suite("Mac switcher")
struct MacSwitcherTests {
    @Test("each Mac in the menu shows its Up next count and how its link reads")
    func detail() {
        #expect(MacMenu.detail(upNext: 2, reachability: .online) == "2 up next · Connected")
        #expect(MacMenu.detail(upNext: 0, reachability: .reconnecting) == "Reconnecting…")
    }

    @Test("the Mac is dotted only when a Mac other than the selected one has sessions up next")
    func othersWaiting() {
        #expect(OtherMacs.waiting(["d1": 0, "d2": 3], selected: "d1"))
        #expect(!OtherMacs.waiting(["d1": 4, "d2": 0], selected: "d1"))
        #expect(!OtherMacs.waiting([:], selected: nil))
    }
}
