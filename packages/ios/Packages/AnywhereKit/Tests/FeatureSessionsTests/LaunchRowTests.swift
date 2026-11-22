import Foundation
import Testing
import Wire
@testable import FeatureSessions
@testable import Stores

@Suite("Launch rows")
struct LaunchRowTests {
    private func creation(_ steps: [String], done: Bool = false, failed: Bool = false, waiting: Bool = false) -> Creation {
        var c = Creation(kind: .session, steps: steps.map { .init(step: $0) }, error: failed ? ServerError(message: "No") : nil, done: done)
        c.waiting = waiting
        return c
    }

    @Test("a launch lists its steps done, the one running, and those pocketd has still to do")
    func rows() {
        let marks = LaunchRow.rows(creation(["prepare", "verify", "fetch"])).map(\.mark)
        #expect(marks == [.done, .done, .running, .later, .later, .later, .later])
        #expect(LaunchRow.rows(creation(["prepare", "verify"], failed: true)).map(\.mark).prefix(2) == [.done, .failed])
        #expect(LaunchRow.rows(nil).map(\.label) == ["Asking your Mac"])
    }

    @Test("the bar fills with the steps done and half the running one, from empty until the Mac reports a step")
    func progress() {
        #expect(LaunchRow.progress(nil) == 0)
        #expect(LaunchRow.progress(creation([])) == 0)
        #expect(LaunchRow.progress(creation(["prepare"])) == 0.5 / 7)
        #expect(LaunchRow.progress(creation(["prepare", "verify", "fetch"])) == 2.5 / 7)
        #expect(LaunchRow.progress(creation(["prepare", "verify", "fetch", "worktree", "copy", "setup", "agent"], done: true)) == 1)
        #expect(LaunchRow.progress(creation(["prepare", "verify"], failed: true)) == 2.0 / 7, "a failed step counts as reached")
    }

    @Test("a launch into an existing place lists only Preparing and Starting agent, and a new worktree drops copy or setup when off")
    func order() {
        let into = LaunchPlan(spec: LaunchSpec(project: "/p", checkout: .worktree("/p"), provider: .claude, access: .edits, plan: false))
        let new = LaunchPlan(spec: LaunchSpec(project: "/p", checkout: .new(.init(name: "otter", setup: false)), provider: .claude, access: .edits, plan: false))
        #expect(LaunchRow.order(Creation(kind: .session, plan: into)) == ["prepare", "agent"])
        #expect(LaunchRow.order(Creation(kind: .session, plan: new)) == ["prepare", "verify", "fetch", "worktree", "copy", "agent"])
        #expect(LaunchRow.rows(Creation(kind: .session, steps: [.init(step: "prepare")], plan: into)).map(\.label) == ["Preparing", "Starting agent"])
    }

    @Test("a done step reads how long it took, up to when the next began; the running one has no time yet")
    func took() {
        let t0 = Date(timeIntervalSince1970: 1_000)
        let c = Creation(kind: .session, steps: [.init(step: "prepare", at: t0), .init(step: "verify", at: t0 + 0.5), .init(step: "fetch", at: t0 + 2.75)])
        #expect(c.took(0) == 0.5)
        #expect(c.took(1) == 2.25)
        #expect(c.took(2) == nil)
        #expect(LaunchRow.rows(c).prefix(3).map(\.took) == [0.5, 2.25, nil])
    }
}
