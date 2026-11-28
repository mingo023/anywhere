import ActivityModel
import DesignSystem
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

    @Test("each state's dot is tinted by its urgency")
    func dots() {
        #expect(ActivityDot.tint(.failed) != ActivityDot.tint(.needsYou))
        #expect(ActivityDot.tint(.needsYou) == Palette.waitingDot)
    }

    @Test("what follows the dot is how long it has waited, or how many are done")
    func readings() throws {
        let states = Dictionary(uniqueKeysWithValues: LiveActivityGallery.states().map { ($0.name, $0.snapshot.state) })
        let needs = try #require(states["needs-you"])
        #expect(ActivityTrailing.reading(needs) == .elapsed(since: try #require(needs.top).since))
        #expect(ActivityTrailing.reading(try #require(states["working"])) == .elapsed(since: try #require(states["working"]?.top).since))
        #expect(ActivityTrailing.reading(try #require(states["all-done"])) == .count(3))
        #expect(ActivityTrailing.reading(.init(working: 0, needsYou: 0, unseen: 0)) == .none)
    }

    @Test("a command shows alone, any other ask led by its verb")
    func commands() {
        let top = { (verb: String?, subject: String?) in
            MacActivity.Top(slot: "a", status: "needsYou", sinceUnix: 0, verb: verb, subject: subject, answerable: true)
        }
        #expect(ActivityText.command(top("Run", "pnpm test")) == "pnpm test")
        #expect(ActivityText.command(top("Edit", "src/a.ts")) == "edit src/a.ts")
        #expect(ActivityText.command(top(nil, "src/a.ts")) == "src/a.ts")
        #expect(ActivityText.command(top("Run", nil)) == nil)
        #expect(ActivityText.command(top("Run", "")) == nil)
    }

    @Test("a place splits into its project and worktree, and anything else stays whole")
    func places() {
        #expect(ActivityText.place("anywhere · flaky-checkout") == ("anywhere", "flaky-checkout"))
        #expect(ActivityText.place("anywhere").worktree == nil)
    }

    @Test("the header names the Mac and how many more want a look, unless it's offline")
    func headers() throws {
        let states = Dictionary(uniqueKeysWithValues: LiveActivityGallery.states().map { ($0.name, $0.snapshot) })
        var needs = try #require(states["needs-you"])
        #expect(ActivityText.header(needs) == "Mingo's MacBook Pro · +2 more")
        needs.isStale = true
        #expect(ActivityText.header(needs) == "Mingo's MacBook Pro")
    }

    @MainActor
    @Test("the gallery renders the design's frames and every state in light and dark")
    func gallery() {
        let names = LiveActivityGallery.scenes.map(\.name)
        #expect(names.count == 16 && names.contains("live-activity-stale-dark") && names.contains("live-activity-needs-you"))
        #expect(["40-push-notification", "41-live-activity", "41b-live-activity-variants"].allSatisfy { names.contains("\($0)-dark") && names.contains("\($0)-light") })
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
