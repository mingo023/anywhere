import ActivityModel
import Foundation
import Testing
@testable import LiveActivityUI

@Suite("Live Activity UI")
struct LiveActivityUITests {
    @Test("Allow and Deny show only for a plain ask while the Mac is reachable")
    func answerable() throws {
        let states = Dictionary(uniqueKeysWithValues: LiveActivityGallery.states().map { ($0.name, $0.snapshot) })
        let needs = try #require(states["needs-you"])
        #expect(needs.answer?.mac == "mingos-mbp" && needs.answer?.request == "r-needs")
        var stale = needs
        stale.isStale = true
        #expect(stale.answer == nil)
        #expect(states["working"]?.answer == nil && states["all-done"]?.answer == nil)
    }

    @Test("the whole activity opens its top session, or the Mac when nothing is on top")
    func links() throws {
        let states = Dictionary(uniqueKeysWithValues: LiveActivityGallery.states().map { ($0.name, $0.snapshot) })
        #expect(states["needs-you"]?.link.absoluteString == "anywhere://mac/mingos-mbp/agent/a-needs")
        #expect(states["all-done"]?.link.absoluteString == "anywhere://mac/mingos-mbp")
    }

    @Test("each state's glyph is a symbol tinted by its urgency")
    func glyphs() {
        #expect(ActivityGlyph.symbol(.needsYou) == "hand.raised.circle.fill")
        #expect(ActivityGlyph.tint(.failed) != ActivityGlyph.tint(.needsYou))
    }

    @MainActor
    @Test("the gallery renders every state in light and dark")
    func gallery() {
        let names = LiveActivityGallery.scenes.map(\.name)
        #expect(names.count == 10 && names.contains("live-activity-stale-dark") && names.contains("live-activity-needs-you"))
    }

    @MainActor
    @Test("an answer goes to the app's handler, and without one it fails")
    func answering() async throws {
        Answering.handler = nil
        await #expect(throws: Answering.Unavailable.self) { try await Answering.answer(mac: "m", request: "r", decision: "allow") }
        nonisolated(unsafe) var got: [String] = []
        Answering.handler = { mac, request, decision in got = [mac, request, decision] }
        try await Answering.answer(mac: "m", request: "r", decision: "deny")
        #expect(got == ["m", "r", "deny"])
        Answering.handler = nil
    }
}
