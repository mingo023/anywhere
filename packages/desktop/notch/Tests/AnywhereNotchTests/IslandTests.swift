@testable import AnywhereNotch
import XCTest

final class IslandTests: XCTestCase {
    func ask(_ request: String, feedback: Bool = true) -> Ask {
        Ask(agent: "agent-\(request)", request: request, provider: "claude", name: "Claude", icon: "", title: "Fix the flaky test",
            place: "app · main", verb: "Wants to run", subject: "ls /tmp", shell: true, feedback: feedback)
    }

    func testANewAskWaitsBehindTheOneShowing() {
        var island = Island()
        island.show([ask("q1")])
        island.open()
        island.show([ask("q1"), ask("q2")])
        XCTAssertEqual(island.current?.request, "q1")
        XCTAssertEqual(island.stage, .open)
    }

    func testAnAskArrivingBehindTheOneShowingChangesTheIsland() {
        var island = Island()
        island.show([ask("q1")])
        let before = island
        island.show([ask("q1"), ask("q2")])
        XCTAssertNotEqual(island, before)
    }

    func testAnAskAnsweredElsewhereLeavesAndTheNextShowsCompact() {
        var island = Island()
        island.show([ask("q1"), ask("q2")])
        island.open()
        island.show([ask("q2")])
        XCTAssertEqual(island.current?.request, "q2")
        XCTAssertEqual(island.stage, .compact)
    }

    func testTheAnsweredAskShowsHowItWentUntilNext() {
        var island = Island()
        island.show([ask("q1"), ask("q2")])
        island.open()
        let answered = island.answer(.deny)
        island.show([ask("q2")])
        XCTAssertEqual(answered?.request, "q1")
        XCTAssertEqual(island.current?.request, "q1")
        XCTAssertEqual(island.stage, .done(.deny))
        island.next()
        XCTAssertEqual(island.current?.request, "q2")
        XCTAssertEqual(island.stage, .compact)
    }

    func testAnAnsweredAskTheDesktopStillListsDoesNotShowAgain() {
        var island = Island()
        island.show([ask("q1")])
        _ = island.answer(.allow)
        island.show([ask("q1")])
        island.next()
        XCTAssertNil(island.current)
    }

    func testAnAskIsAnsweredOnce() {
        var island = Island()
        island.show([ask("q1")])
        let first = island.answer(.allow)
        let again = island.answer(.deny)
        XCTAssertNotNil(first)
        XCTAssertNil(again)
        XCTAssertEqual(island.stage, .done(.allow))
    }

    func testReplyNeedsAnOpenAskThatTakesFeedback() {
        var island = Island()
        island.show([ask("q1", feedback: false)])
        island.reply()
        XCTAssertEqual(island.stage, .compact)
        island.open()
        island.reply()
        XCTAssertEqual(island.stage, .open)
    }

    func testClosingStepsBackFromTheReplyToTheAskToCompact() {
        var island = Island()
        island.show([ask("q1")])
        island.open()
        island.reply()
        island.close()
        XCTAssertEqual(island.stage, .open)
        island.close()
        XCTAssertEqual(island.stage, .compact)
    }

    func testCollapsingGoesFromTheReplyStraightToCompact() {
        var island = Island()
        island.show([ask("q1")])
        island.open()
        island.reply()
        island.collapse()
        XCTAssertEqual(island.stage, .compact)
    }

    func testAnAnsweredAskIsForgottenOnceTheDesktopDropsIt() {
        var island = Island()
        island.show([ask("q1")])
        _ = island.answer(.allow)
        island.next()
        island.show([])
        island.show([ask("q1")])
        XCTAssertEqual(island.current?.request, "q1")
    }
}
