import ChatModel
import Fixtures
import Stores
import Testing
import Wire
@testable import FeatureChat

@MainActor
private func rows(_ s: ChatStore) -> [RowItem] {
    let b = RowBuilder(
        session: s.session, provider: s.provider, model: s.summary?.model, ask: s.ask.map { !$0.questions.isEmpty }, stopping: s.stopping,
        canSendNow: true, now: Sample.now)
    return s.session.rows.indices.map(b.item)
}

private func shown(_ items: [RowItem]) -> [RowContent] {
    items.map(\.content).filter { $0 != .hidden }
}

@Suite("Chat rows")
@MainActor
struct FeatureChatTests {
    @Test("a shut fold hides the work and keeps the answer")
    func shutFold() {
        let all = shown(rows(ChatGallery.store(Sample.chat("every-tool-kind"))))
        #expect(!all.contains { if case .phase = $0 { true } else { false } })
        #expect(!all.contains { if case .tasks = $0 { true } else { false } })
        #expect(all.contains { if case .text(let t) = $0 { t.text.hasPrefix("Tool runs now fold") } else { false } })
        #expect(all.contains { if case .fold(let label, false, _) = $0 { label == "Claude worked for 1m 4s" } else { false } })
    }

    @Test("an open fold hangs its work on the rail, whose last row stops at its elbow")
    func openFold() {
        let s = ChatGallery.store(Sample.chat("every-tool-kind"))
        let fold = s.session.ids.first { $0.hasSuffix("#fold") }!
        s.toggle(fold, false)
        let items = rows(s)
        let railed = items.filter { $0.rail != .none }
        #expect(!railed.isEmpty)
        #expect(railed.allSatisfy { if case .phase = $0.content { true } else if case .text = $0.content { true } else { false } })
        #expect(items.contains { $0.rail == .last })
        #expect(items.contains { if case .tasks = $0.content { true } else { false } })
    }

    @Test("the live reply's fold gives way to the live line at the bottom")
    func liveTail() {
        let items = rows(ChatGallery.store(Sample.chat("streaming-live")))
        #expect(items.contains { $0.id.hasSuffix("#fold") && $0.content == .hidden })
        guard case .live(let live) = items.last?.content else { return #expect(Bool(false), "the last row is the live line") }
        #expect(live.label(now: (live.start ?? 0) + 7_000).hasSuffix("working for 7s"))
    }

    @Test("a steer under the reply says when the agent reads it")
    func steer() {
        let s = ChatGallery.store(Sample.chat("streaming-live"), status: .working)
        s.draft.text = "also the tests"
        s.send()
        s.flush()
        guard case .pending(let p)? = shown(rows(s)).first(where: { if case .pending = $0 { true } else { false } }) else {
            return #expect(Bool(false), "the steer shows as a pending bubble")
        }
        #expect(p.delivery == .steered)
        #expect(p.waiting == "Claude reads this after its current step")
    }

    @Test("a settled reply's footer copies only the answer and retries its prompt")
    func footer() {
        let items = shown(rows(ChatGallery.store(Sample.chat("every-tool-kind"))))
        guard case .end(.done(let footer?))? = items.last else { return #expect(Bool(false), "the reply ends on its footer") }
        #expect(footer.answer.hasPrefix("## Plan") && footer.answer.contains("Tool runs now fold"))
        #expect(!footer.answer.contains("I'll start by reading"), "narration between the work is not the answer")
        #expect(footer.retry == 0)
        #expect(footer.stats.hasPrefix("1m 4s · "))
    }

    @Test("a failed turn shows its error and a stopped one asks what to do instead")
    func ends() {
        let ends = shown(rows(ChatGallery.store(Sample.chat("repro-ingestion-interrupt-then-done")))).compactMap { c -> EndContent? in
            if case .end(let e) = c { e } else { nil }
        }
        #expect(ends.contains(.interrupted("Interrupted · What should Claude do instead?")))
    }

    @Test("every scenario has a light and a dark scene")
    func scenes() {
        let names = ChatGallery.scenes.map(\.name)
        for scenario in Sample.chatScenarios {
            #expect(names.contains("chat-\(scenario.name)") && names.contains("chat-\(scenario.name)-dark"))
        }
    }
}

@Test("a row on screen moving down means the list scrolled up; the last row growing or the end moving doesn't")
func scrollingUpIsARowMovingDown() {
    let before = [RowFrame(ix: 3, minY: 100, maxY: 300), RowFrame(ix: 4, minY: 300, maxY: 700), RowFrame(ix: RowFrame.end, minY: 700, maxY: 700)]
    let seen = RowFrame.seen(before, viewport: 800)
    #expect(seen?.ix == 4)
    let grew = [RowFrame(ix: 3, minY: 100, maxY: 300), RowFrame(ix: 4, minY: 300, maxY: 900), RowFrame(ix: RowFrame.end, minY: 900, maxY: 900)]
    #expect(!RowFrame.scrolledUp(from: seen, to: grew))
    let up = before.map { RowFrame(ix: $0.ix, minY: $0.minY + 30, maxY: $0.maxY + 30) }
    #expect(RowFrame.scrolledUp(from: seen, to: up))
    #expect(!RowFrame.scrolledUp(from: nil, to: up))
}
