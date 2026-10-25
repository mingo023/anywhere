import Domain
import Foundation
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
        nav.tab = .terminals
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
        #expect(gone.tab == .terminals)
    }
}
