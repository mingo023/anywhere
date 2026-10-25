import Foundation
import Testing
import Wire
@testable import Fixtures

@Suite("Fixtures")
struct FixturesTests {
    @Test("the sample agents cover every status a card can show")
    func statuses() {
        let statuses = Set(Sample.agents.map(\.status))
        #expect(statuses.isSuperset(of: [.needsYou, .done, .working, .idle]))
        #expect(Sample.agents.contains { $0.failed == true })
        #expect(Sample.agents.contains { !$0.attached })
        #expect(Set(Sample.agents.map(\.id)).count == Sample.agents.count)
    }

    @Test("the sample timeline holds every item kind and tool kind")
    func timelineKinds() {
        let items = Sample.timeline().items
        let kinds = Set(items.map(\.body.kind))
        #expect(kinds == ["user", "assistant", "thinking", "tool", "tasks", "plan", "compact", "result", "notice"])
        let tools = Set(items.compactMap { item -> String? in
            if case .tool(let call) = item.body { call.detail.kind } else { nil }
        })
        #expect(tools == ["shell", "read", "edit", "write", "search", "task", "other"])
        #expect(items.contains { $0.parentToolUseId != nil })
        #expect(items.map(\.seq) == Array(1...items.count))
    }

    @Test("the sample git status has 1200 sorted files, and the diff the rows asked for")
    func git() {
        let status = Sample.gitStatus()
        #expect(status.files.count == 1200)
        #expect(status.files.map(\.path) == status.files.map(\.path).sorted())
        #expect(Set(status.files.map(\.status)) == [.modified, .added, .deleted, .renamed, .untracked, .conflict])
        #expect(Sample.diff(lines: 6000).lines.count == 6000)
    }

    @Test("every sample message survives the wire")
    func encodes() throws {
        let messages = Sample.snapshot + [.agentTimeline(Sample.timeline(turns: 8)), .gitStatus(Sample.gitStatus(files: 50)), .gitDiff(Sample.diff(lines: 200))]
        for message in messages {
            let frame = ServerFrame(id: "x", message)
            #expect(try ServerFrame.decode(frame.encoded()) == frame)
        }
    }
}
