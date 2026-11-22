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
}
