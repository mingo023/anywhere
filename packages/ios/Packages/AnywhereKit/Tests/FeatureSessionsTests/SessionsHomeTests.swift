import Domain
import Fixtures
import Foundation
@testable import Stores
import Testing
import Wire
@testable import FeatureSessions

private let utc = TimeZone(secondsFromGMT: 0)!

@Suite("Sessions home")
struct SessionsHomeTests {
    private func card(_ id: String, _ status: Status, createdAt: Int = 0, updatedAt: Int, pinned: Bool = false) -> Sidebar.Card {
        Sidebar.Card(id: id, title: id, status: status, createdAt: createdAt, updatedAt: updatedAt, pinned: pinned)
    }

    @Test("All and Working list the sessions working now, and Pinned lists the pinned ones whatever they are doing")
    func filters() {
        let cards = [card("a", .working, updatedAt: 1), card("b", .working, updatedAt: 3), card("c", .done, updatedAt: 9, pinned: true), card("d", .idle, updatedAt: 5)]
        #expect(SessionsFilter.all.sessions(cards, by: .activity) == ["b", "a"])
        #expect(SessionsFilter.working.sessions(cards, by: .activity) == ["b", "a"])
        #expect(SessionsFilter.pinned.sessions(cards, by: .activity) == ["c"])
    }

    @Test("the sessions listed follow the order picked in the Sessions menu")
    func sortOrder() {
        let cards = [card("b", .working, createdAt: 2, updatedAt: 1), card("a", .working, createdAt: 1, updatedAt: 3)]
        #expect(SessionsFilter.all.sessions(cards, by: .newest) == ["b", "a"])
        #expect(SessionsFilter.all.sessions(cards, by: .activity) == ["a", "b"])
        #expect(SessionsFilter.all.sessions(cards, by: .name) == ["a", "b"])
    }

    @Test("only All lists projects")
    func projects() {
        #expect(SessionsFilter.all.showsProjects)
        #expect(!SessionsFilter.working.showsProjects)
        #expect(!SessionsFilter.pinned.showsProjects)
    }

    @Test("a Mac's line says how it is reached, or when it was last seen")
    func connection() {
        let asOf = 1_792_832_400_000
        #expect(SessionsText.connection(.online, asOf: nil, timeZone: utc) == "Connected over Tailscale")
        #expect(SessionsText.connection(.reconnecting, asOf: asOf, timeZone: utc) == "Reconnecting…")
        #expect(SessionsText.connection(.unreachable, asOf: asOf, timeZone: utc) == "Offline · last seen 9:00")
        #expect(SessionsText.connection(.stopped, asOf: nil, timeZone: utc) == "Offline")
        #expect(SessionsText.connection(.revoked, asOf: asOf, timeZone: utc) == Reachability.revoked.label)
    }

    @Test("a Mac that can't be reached has a grey dot, not a red one")
    func dot() {
        #expect(SessionsText.dot(.online) == .up)
        #expect(SessionsText.dot(.reconnecting) == .trying)
        #expect(SessionsText.dot(.unreachable) == .off)
        #expect(SessionsText.dot(.revoked) == .lost)
    }
}

@Suite("Tree text")
struct TreeTextTests {
    private func item(_ gate: Gate, state: String = "OPEN", draft: Bool = false) -> PrItem {
        PrItem(tree: "/t", number: 42, title: "", state: state, draft: draft, gate: gate, checks: .init(passed: 3, failed: gate == .checksFailed ? 1 : 0, pending: 0))
    }

    @Test("a pull request reads as its number and what it waits on, in the tone of that wait")
    func pullRequest() {
        #expect(TreeText.pullRequest(item(.checksFailed)).text == "#42 · Checks failing")
        #expect(TreeText.pullRequest(item(.checksFailed)).tone == .failed)
        #expect(TreeText.pullRequest(item(.ready)).text == "#42 · Ready to merge")
        #expect(TreeText.pullRequest(item(.ready)).tone == .success)
        #expect(TreeText.pullRequest(item(.ready, state: "MERGED")).text == "#42 · Merged")
        #expect(TreeText.pullRequest(item(.ready, draft: true)).text == "#42 · Draft")
        #expect(TreeText.pullRequest(item(.blocked)).text == "#42 · Open")
    }

    @Test("a branch stands ahead, behind or up to date once git has answered, and says nothing before")
    func standing() {
        var head = GitHead(Sample.gitStatus(files: 1))
        #expect(TreeText.standing(head) == "4 ahead")
        head.ahead = 0
        head.behind = 2
        #expect(TreeText.standing(head) == "2 behind")
        head.behind = 0
        #expect(TreeText.standing(head) == "up to date")
        #expect(TreeText.standing(nil) == nil)
    }

    @Test("an offline Mac's tree says when its data is from and what is off")
    func offlineNote() {
        #expect(TreeText.offlineNote(asOf: 1_792_832_400_000, timeZone: utc) == "Offline · as of 9:00. New sessions and changes are off.")
    }
}
