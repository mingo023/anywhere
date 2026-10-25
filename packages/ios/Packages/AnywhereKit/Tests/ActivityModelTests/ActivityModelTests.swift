import Foundation
import Testing
@testable import ActivityModel

/// pocketd's Live Activity payloads, from its push testdata.
enum PocketdPayload {
    static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("pocketd/internal/push/testdata")

    struct Aps<State: Decodable>: Decodable {
        struct Inner: Decodable {
            let event: String
            let contentState: State
            let attributes: MacActivity?
            let relevanceScore: Double?
            let staleDate: Double?
            let dismissalDate: Double?
            let attributesType: String?

            enum CodingKeys: String, CodingKey {
                case event, attributes
                case contentState = "content-state"
                case relevanceScore = "relevance-score"
                case staleDate = "stale-date"
                case dismissalDate = "dismissal-date"
                case attributesType = "attributes-type"
            }
        }

        let aps: Inner
    }

    static func load(_ name: String) throws -> Aps<MacActivity.ContentState>.Inner {
        try JSONDecoder().decode(Aps<MacActivity.ContentState>.self, from: Data(contentsOf: root.appendingPathComponent(name))).aps
    }
}

@Suite("Mac activity")
struct ActivityModelTests {
    @Test("pocketd's update decodes with ActivityKit's default decoder, its time as Unix seconds")
    func decodesUpdate() throws {
        let aps = try PocketdPayload.load("activity_update.json")
        let top = try #require(aps.contentState.top)
        #expect(aps.event == "update" && aps.contentState == MacActivity.ContentState(working: 2, needsYou: 1, unseen: 1, top: top))
        #expect(top == MacActivity.Top(slot: "k7", status: "needsYou", sinceUnix: 1_761_299_950.25, verb: "Run", subject: "npm test", answerable: true, request: "r_8f2"))
        #expect(top.since == Date(timeIntervalSince1970: 1_761_299_950.25))
        #expect(aps.relevanceScore == aps.contentState.relevance && aps.staleDate == 1_761_300_300)
    }

    @Test("pocketd's push-to-start carries the attributes type and attributes the app declares")
    func decodesStart() throws {
        let aps = try PocketdPayload.load("activity_start.json")
        #expect(aps.event == "start" && aps.attributesType == String(describing: MacActivity.self))
        #expect(aps.attributes == MacActivity(mac: "studio", macName: "Studio"))
    }

    @Test("pocketd's end leaves nothing on top and says when to dismiss")
    func decodesEnd() throws {
        let aps = try PocketdPayload.load("activity_end.json")
        #expect(aps.event == "end" && aps.contentState == MacActivity.ContentState(working: 0, needsYou: 0, unseen: 3) && aps.dismissalDate == 1_761_300_900)
    }

    @Test("a state with nothing on top leaves it out")
    func noTop() throws {
        let state = MacActivity.ContentState(working: 0, needsYou: 0, unseen: 0)
        let json = try JSONDecoder().decode([String: Int].self, from: JSONEncoder().encode(state))
        #expect(json == ["working": 0, "needsYou": 0, "unseen": 0])
    }

    @Test("the app builds the state pocketd would: counts, and the oldest Needs you on top")
    func builds() {
        let sessions = [
            ActivitySession(id: "w1", status: "working", updatedAt: 3000), ActivitySession(id: "w2", status: "working", updatedAt: 2000),
            ActivitySession(id: "n2", status: "needsYou", updatedAt: 5000), ActivitySession(id: "n1", status: "needsYou", updatedAt: 4000),
            ActivitySession(id: "d", status: "done", updatedAt: 500), ActivitySession(id: "f", status: "failed", updatedAt: 1000),
            ActivitySession(id: "i", status: "idle", updatedAt: 1),
        ]
        let ask = ActivityAsk(requestId: "r1", agentId: "n1", verb: "Run", subject: "npm test", answerable: true)
        let state = MacActivity.ContentState.build(sessions, asks: [ask], details: false)
        #expect(state == MacActivity.ContentState(working: 2, needsYou: 2, unseen: 2, top: MacActivity.Top(slot: "n1", status: "needsYou", sinceUnix: 4, answerable: true, request: "r1")))
        #expect(state.relevance == 100)
        let detailed = MacActivity.ContentState.build(sessions, asks: [ask], details: true)
        #expect(detailed.top?.verb == "Run" && detailed.top?.subject == "npm test")
    }

    @Test("without Needs you the longest working is on top, then the oldest Failed, then the oldest Done")
    func topOrder() {
        let f = ActivitySession(id: "f", status: "failed", updatedAt: 1000)
        #expect(MacActivity.ContentState.build([.init(id: "w1", status: "working", updatedAt: 3000), .init(id: "w2", status: "working", updatedAt: 2000), f], asks: [], details: false).top?.slot == "w2")
        #expect(MacActivity.ContentState.build([.init(id: "d", status: "done", updatedAt: 500), f], asks: [], details: false).top?.slot == "f")
        #expect(MacActivity.ContentState.build([.init(id: "d2", status: "done", updatedAt: 900), .init(id: "d1", status: "done", updatedAt: 500)], asks: [], details: false).top?.slot == "d1")
        let long = ActivityAsk(requestId: "r", agentId: "n", verb: "Run", subject: String(repeating: "x", count: 80), answerable: false)
        let top = MacActivity.ContentState.build([.init(id: "n", status: "needsYou", updatedAt: 1)], asks: [long], details: true).top
        #expect(top?.subject?.count == 60 && top?.answerable == false && top?.request == nil)
    }

    @Test("each region says what the research's layout asks of it")
    func display() {
        let needs = MacActivity.ContentState(working: 1, needsYou: 2, unseen: 0, top: .init(slot: "a", status: "needsYou", sinceUnix: 10, verb: "Run", subject: "npm test", answerable: true, request: "r"))
        #expect(ActivityDisplay.glyph(needs) == .needsYou && ActivityDisplay.trailing(needs) == .needs(2) && ActivityDisplay.badge(needs) == 2)
        #expect(ActivityDisplay.ask(needs.top!) == "Run · npm test" && ActivityDisplay.more(needs) == "+2 more")
        #expect(ActivityDisplay.summary(needs) == "2 need you · 1 working")
        let working = MacActivity.ContentState(working: 2, needsYou: 0, unseen: 0, top: .init(slot: "w", status: "working", sinceUnix: 100, answerable: false))
        #expect(ActivityDisplay.trailing(working) == .timer(since: Date(timeIntervalSince1970: 100)) && ActivityDisplay.badge(working) == 2)
        #expect(ActivityDisplay.ask(working.top!) == "Working")
        let done = MacActivity.ContentState(working: 0, needsYou: 0, unseen: 3)
        #expect(ActivityDisplay.glyph(done) == .idle && ActivityDisplay.trailing(done) == .unseen(3) && ActivityDisplay.badge(done) == nil)
        #expect(ActivityDisplay.summary(.init(working: 0, needsYou: 0, unseen: 0)) == "All done")
        #expect(ActivityDisplay.offline("Studio") == "Studio is offline")
        #expect(ActivityDisplay.link(mac: "m", top: needs.top).absoluteString == "anywhere://mac/m/agent/a")
    }

    @Test("titles the app saved name the top session; a slot it never saw reads as a session")
    func titles() throws {
        let suite = "dev.mingo.anywhere.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let titles = ActivityTitles(defaults: defaults)
        titles.save(mac: "m", ["a": .init(title: "Fix the flaky test", place: "anywhere · calm-otter")])
        #expect(titles.entry(mac: "m", slot: "a").title == "Fix the flaky test")
        #expect(titles.entry(mac: "m", slot: "b") == ActivityTitles.fallback)
        #expect(titles.entry(mac: "other", slot: "a").title == "A session")
    }
}
