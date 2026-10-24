import Testing
@testable import Domain

private func r(_ id: String, _ status: Status, updated: Int, created: Int = 0) -> Ranked {
    Ranked(id: id, status: status, updatedAt: updated, createdAt: created)
}

@Suite("Status and Up next")
struct StatusTests {
    @Test("a session's status comes from its agent, and an agent that isn't attached is idle")
    func fromAgent() {
        #expect(Status(status: "needsYou", failed: false, attached: true) == .needsYou)
        #expect(Status(status: "done", failed: true, attached: true) == .failed)
        #expect(Status(status: "done", failed: false, attached: true) == .done)
        #expect(Status(status: "working", failed: false, attached: true) == .working)
        #expect(Status(status: "idle", failed: false, attached: true) == .idle)
        #expect(Status(status: "closed", failed: false, attached: true) == .idle)
        #expect(Status(status: "needsYou", failed: false, attached: false) == .idle)
    }

    @Test("statuses rank Needs you, Failed, Done, Working, Idle")
    func ranks() {
        #expect(Status.allCases.sorted() == [.needsYou, .failed, .done, .working, .idle])
        #expect(Status.allCases.filter(\.wantsLook) == [.needsYou, .failed, .done])
    }

    @Test("Up next is Needs you, then Failed, then Done, each oldest status change first")
    func upNext() {
        let sessions = [
            r("working", .working, updated: 1), r("done-new", .done, updated: 50), r("needs-new", .needsYou, updated: 40),
            r("failed", .failed, updated: 60), r("done-old", .done, updated: 10), r("needs-old", .needsYou, updated: 30),
            r("idle", .idle, updated: 0),
        ]
        #expect(Order.upNext(sessions) == ["needs-old", "needs-new", "failed", "done-old", "done-new"])
    }

    @Test("ties in Up next fall back to the id, so the order never flickers")
    func ties() {
        #expect(Order.upNext([r("b", .done, updated: 5), r("a", .done, updated: 5)]) == ["a", "b"])
    }

    @Test("every session lists newest first")
    func recent() {
        #expect(Order.recent([r("a", .idle, updated: 0, created: 1), r("b", .idle, updated: 0, created: 3), r("c", .idle, updated: 0, created: 2)]) == ["b", "c", "a"])
    }

    @Test("the summary counts what wants a look and what works, else the sessions")
    func summary() {
        #expect(Order.summary([]) == nil)
        #expect(Order.summary([.needsYou, .needsYou, .failed, .working]) == "2 need you · 1 failed · 1 working")
        #expect(Order.summary([.needsYou, .done]) == "1 needs you · 1 done")
        #expect(Order.summary([.idle]) == "1 session")
        #expect(Order.summary([.idle, .idle]) == "2 sessions")
    }
}
