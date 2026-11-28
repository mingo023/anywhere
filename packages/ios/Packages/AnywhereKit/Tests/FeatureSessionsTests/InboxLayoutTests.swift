import DesignSystem
import Domain
import Fixtures
import Foundation
@testable import Stores
import Testing
import Wire
@testable import FeatureSessions

@Suite("Inbox layout")
struct InboxLayoutTests {
    private func session(_ id: String, _ status: Status, _ updatedAt: Int) -> InboxLayout.Session {
        InboxLayout.Session(id: id, status: status, updatedAt: updatedAt)
    }

    private func mac(_ id: String, _ sessions: [InboxLayout.Session], _ reachability: Reachability = .online, asOf: Int? = nil) -> InboxLayout.Mac {
        InboxLayout.Mac(id: id, name: id, reachability: reachability, asOf: asOf, sessions: sessions)
    }

    @Test("sessions fall into Needs you, Failed, Finished and Working across Macs, newest first, and idle ones are left out")
    func groups() {
        let built = InboxLayout.build([
            mac("book", [session("a", .needsYou, 100), session("b", .done, 300), session("c", .idle, 400), session("d", .working, 50)]),
            mac("mini", [session("e", .needsYou, 200), session("f", .failed, 10), session("g", .done, 500)]),
        ])
        #expect(built.needsYou.map(\.key) == ["mini/e", "book/a"])
        #expect(built.failed.map(\.key) == ["mini/f"])
        #expect(built.finished.map(\.key) == ["mini/g", "book/b"])
        #expect(built.working.map(\.key) == ["book/d"])
        #expect(!built.isEmpty)
        #expect(InboxLayout.build([mac("book", [session("c", .idle, 1)])]).isEmpty)
    }

    @Test("the subtitle counts what needs you, failed and finished, leaving out what is zero")
    func subtitle() {
        #expect(InboxLayout.subtitle(needsYou: 2, failed: 1, finished: 1) == "2 need you · 1 failed · 1 finished")
        #expect(InboxLayout.subtitle(needsYou: 1, failed: 0, finished: 3) == "1 needs you · 3 finished")
        #expect(InboxLayout.subtitle(needsYou: 0, failed: 0, finished: 0) == nil)
    }

    @Test("a strip stands in for the subtitle")
    func stripReplacesSubtitle() {
        let calm = InboxLayout.build([mac("book", [session("a", .needsYou, 1)])])
        #expect(calm.subtitle == "1 needs you")
        let gone = InboxLayout.build([mac("book", [session("a", .needsYou, 1)], .reconnecting)])
        #expect(gone.strip != nil)
        #expect(gone.subtitle == nil)
    }

    @Test("a Mac that is dialling dims its own rows and locks their answers, and one that is online or only blinked doesn't")
    func reconnecting() throws {
        let built = InboxLayout.build([
            mac("book", [session("a", .needsYou, 2)], .reconnecting), mac("mini", [session("b", .needsYou, 1)], .online),
            mac("blip", [session("c", .needsYou, 0)], .connecting),
        ])
        let book = try #require(built.needsYou.first { $0.mac == "book" })
        #expect(book.dim == 0.5 && book.locked)
        for other in built.needsYou where other.mac != "book" { #expect(other.dim == 1 && !other.locked) }
        let strip = try #require(built.strip)
        #expect(strip.kind == .reconnecting)
        #expect(strip.text == "Reconnecting to book…")
        #expect(!strip.retry)
    }

    @Test("a Mac that can't be reached says when its data is from and offers Retry")
    func unreachable() throws {
        let asOf = 1_792_832_400_000
        let built = InboxLayout.build([mac("book", [session("a", .needsYou, 1)], .unreachable, asOf: asOf)])
        let strip = try #require(built.strip)
        #expect(strip.kind == .failed)
        #expect(strip.retry)
        #expect(strip.text == NetStrip.failed(mac: "book", asOf: asOf))
        #expect(built.needsYou[0].locked && built.needsYou[0].dim == 0.5)
    }

    @Test("an offline Mac dims its rows further, and says answers are off only when it holds one that needs you")
    func offline() throws {
        let asOf = 1_792_832_400_000
        let asking = InboxLayout.build([mac("mini", [session("a", .needsYou, 1)], .stopped, asOf: asOf)])
        #expect(asking.strip?.kind == .offline)
        #expect(asking.strip?.text.hasSuffix(" · answers disabled") == true)
        #expect(asking.needsYou[0].dim == 0.42 && asking.needsYou[0].locked)
        let failed = InboxLayout.build([mac("mini", [session("a", .failed, 1)], .stopped, asOf: asOf)])
        #expect(failed.strip?.text.hasSuffix("answers disabled") == false)
    }

    @Test("back online shows only while no Mac is still away")
    func backOnline() {
        #expect(InboxLayout.build([mac("book", [session("a", .done, 1)])], back: true).strip?.kind == .back)
        #expect(InboxLayout.build([mac("book", [session("a", .done, 1)])]).strip == nil)
        #expect(InboxLayout.build([mac("book", [], .reconnecting)], back: true).strip?.kind == .reconnecting)
    }
}

@Suite("Inbox text")
struct InboxTextTests {
    private let agent = Sample.agents.first { $0.id == "a-needs" }!

    @Test("a plain ask leads with its command or path in code, and anything else with its words or the session's title")
    func headline() {
        let shell = InboxText.headline(agent, .needsYou, ask: Sample.permission)
        #expect(shell.text == "rm -rf build" && shell.code)
        let read = PermissionRequest(requestId: "r", agentId: "a", toolName: "Read", detail: .read(path: "src/app.swift"))
        #expect(InboxText.headline(agent, .needsYou, ask: read) == ("src/app.swift", true))
        let question = InboxText.headline(agent, .needsYou, ask: Sample.question)
        #expect(question.text == "Which colour?" && !question.code)
        #expect(InboxText.headline(agent, .needsYou, ask: nil) == (SessionText.title(agent), false))
        #expect(InboxText.headline(agent, .failed, ask: Sample.permission) == (SessionText.title(agent), false))
    }

    @Test("a row's place names the project, the branch in code, and the Mac only when told, skipping what is missing")
    func place() {
        #expect(InboxText.place(project: "anywhere", branch: "flaky-checkout", mac: "MacBook Pro")
            == [.init(text: "anywhere", code: false), .init(text: "flaky-checkout", code: true), .init(text: "MacBook Pro", code: false)])
        #expect(InboxText.place(project: "anywhere", branch: "", mac: nil) == [.init(text: "anywhere", code: false)])
        #expect(InboxText.place(project: nil, branch: nil, mac: nil).isEmpty)
    }

    @Test("an answer sent says where it went, and a denial only that it was denied")
    func answered() {
        #expect(InboxText.answered(.allow, mac: "MacBook Pro") == "Approved · sent to MacBook Pro")
        #expect(InboxText.answered(.deny, mac: "MacBook Pro") == "Denied")
    }
}
