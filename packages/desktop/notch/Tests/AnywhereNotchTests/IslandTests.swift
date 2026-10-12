@testable import AnywhereNotch
import XCTest

final class IslandTests: XCTestCase {
    func ask(_ request: String, feedback: Bool = true, questions: [Question] = []) -> Ask {
        Ask(agent: "agent-\(request)", request: request, provider: "claude", name: "Claude", icon: "", title: "Fix the flaky test",
            place: "app · main", verb: "Wants to run", subject: "ls /tmp", shell: true, feedback: feedback, questions: questions)
    }

    func question(_ text: String, _ labels: [String], multi: Bool = false) -> Question {
        Question(question: text, header: "H", options: labels.map { Choice(label: $0, description: "") }, multiSelect: multi)
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

    func testPickingEachSingleSelectQuestionAnswersTheAsk() {
        var island = Island()
        island.show([ask("q1", questions: [question("Color?", ["Red", "Blue"]), question("Size?", ["S", "M"])])])
        island.open()
        XCTAssertNil(island.pick("Blue"))
        XCTAssertEqual(island.picks.question?.question, "Size?")
        let answered = island.pick("M")
        XCTAssertEqual(answered?.request, "q1")
        XCTAssertEqual(island.stage, .done(.questions(["Color?": "Blue", "Size?": "M"])))
    }

    func testAMultiSelectQuestionTogglesUntilSubmittedInTheOptionsOrder() {
        var island = Island()
        island.show([ask("q1", questions: [question("Tools?", ["Go", "Rust", "Swift"], multi: true)])])
        island.open()
        XCTAssertNil(island.nextQuestion())
        _ = island.pick("Swift")
        _ = island.pick("Rust")
        _ = island.pick("Go")
        _ = island.pick("Rust")
        XCTAssertEqual(island.stage, .open)
        _ = island.nextQuestion()
        XCTAssertEqual(island.stage, .done(.questions(["Tools?": "Go, Swift"])))
    }

    func testGoingBackKeepsTheEarlierPick() {
        var island = Island()
        island.show([ask("q1", questions: [question("Color?", ["Red", "Blue"]), question("Size?", ["S", "M"])])])
        island.open()
        _ = island.pick("Red")
        island.previousQuestion()
        XCTAssertEqual(island.picks.question?.question, "Color?")
        XCTAssertTrue(island.picks.isChosen("Red"))
    }

    func testPicksNeedTheAskOpen() {
        var island = Island()
        island.show([ask("q1", questions: [question("Color?", ["Red"])])])
        XCTAssertNil(island.pick("Red"))
        XCTAssertEqual(island.stage, .compact)
    }

    func testTheNextAskStartsWithNoPicks() {
        var island = Island()
        let colors = [question("Color?", ["Red", "Blue"]), question("Size?", ["S"])]
        island.show([ask("q1", questions: colors), ask("q2", questions: colors)])
        island.open()
        _ = island.pick("Red")
        island.show([ask("q2", questions: colors)])
        XCTAssertEqual(island.current?.request, "q2")
        XCTAssertEqual(island.picks.page, 0)
        XCTAssertFalse(island.picks.isChosen("Red"))
    }

    func testTheDesktopsQuestionsDecode() throws {
        let line = #"{"asks":[{"agent":"a","request":"q","provider":"claude","name":"Claude","icon":"","title":"t","place":"p","verb":"v","subject":"s","shell":false,"feedback":true,"questions":[{"question":"Color?","header":"Color","options":[{"label":"Red","description":"Warm"}],"multiSelect":false}]}]}"#
        let snapshot = try JSONDecoder().decode(Snapshot.self, from: Data(line.utf8))
        XCTAssertEqual(snapshot.asks[0].questions, [Question(question: "Color?", header: "Color", options: [Choice(label: "Red", description: "Warm")], multiSelect: false)])
    }
}
