import Foundation
import Testing
import Wire
@testable import FeatureSessions
@testable import Stores

@Suite("Launch sheet")
struct LaunchSheetTests {
    @Test("chips fill a line until the next one doesn't fit, then wrap")
    func flowWraps() {
        let lines = FlowLayout.lines(widths: [100, 80, 120, 50, 90], in: 300, spacing: 8)
        #expect(lines == [0..<2, 2..<5])
        #expect(FlowLayout.lines(widths: [100, 100, 100], in: 300, spacing: 8) == [0..<2, 2..<3], "the spacing counts toward the width")
    }

    @Test("a chip wider than the line still gets its own line, and nothing makes no lines")
    func flowEdges() {
        #expect(FlowLayout.lines(widths: [400, 50], in: 300, spacing: 8) == [0..<1, 1..<2])
        #expect(FlowLayout.lines(widths: [], in: 300, spacing: 8).isEmpty)
    }

    @Test("the sheet names what it starts by the prompt's first line, else by where it starts")
    func subtitle() {
        var draft = NewSessionDraft(project: "/p", place: .newWorktree)
        draft.name = "dark-mode"
        draft.base = "main"
        let plan = draft.plan(taken: [], localId: "", localName: "", neverAsks: false)
        var creation = Creation(kind: .session, plan: plan, title: "Add dark mode")
        #expect(LaunchProgress.subtitle(creation, project: "p") == "Add dark mode")
        creation.title = ""
        #expect(LaunchProgress.subtitle(creation, project: "p") == "dark-mode from main")
        #expect(LaunchProgress.subtitle(nil, project: nil) == nil)
    }
}
