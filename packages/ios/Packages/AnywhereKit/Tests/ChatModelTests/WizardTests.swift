import Testing
@testable import ChatModel

private func question(_ text: String, _ options: Int, _ multi: Bool) -> Question {
    Question(header: "Pick", question: text, options: (1...options).map { Choice(label: "o\($0)") }, multi: multi)
}

private func wizard(_ questions: [Question]) -> Wizard {
    Wizard(Ask(requestId: "r", tool: .question(questions), toolName: "AskUserQuestion", questions: questions))
}

@Test("eyebrow labels are capitals spaced by hair spaces")
func eyebrowLabels() {
    #expect(trackedUpper("Db") == "D\u{200A}B")
}

@Test("a question titles with its first line and shows the rest below")
func aQuestionTitles() {
    #expect(titleAndBody("Which?") == ("Which?", nil))
    #expect(titleAndBody("Ship this plan?\n\n- one\n- two\n") == ("Ship this plan?", "- one\n- two"))
}

@Test("a single select pick advances after 220ms")
func aSingleSelectPickAdvances() {
    #expect(Wizard.autoAdvance == 0.22)
    var w = wizard([question("Which?", 2, false), question("Many?", 2, true)])
    let first = w.select(1)
    #expect(first == .autoAdvance)
    _ = w.advance()
    let second = w.select(0)
    #expect(second == .stay)
}

@Test("the last page offers submit and waits for it")
func theLastPageOffersSubmit() {
    var w = wizard([question("A?", 2, false), question("B?", 2, false)])
    #expect(w.primary == "Next")
    let next = w.advance()
    #expect(next == .stay)
    #expect(w.primary == "Submit")
    let pick = w.select(0)
    #expect(pick == .stay)
    guard case .done = w.advance() else { Issue.record("the last page submits"); return }
}

@Test("going back from the first page has nowhere to go")
func goingBackFromTheFirstPage() {
    var w = wizard([question("A?", 2, false), question("B?", 2, false)])
    let first = w.back()
    #expect(!first)
    _ = w.advance()
    #expect(w.forward)
    let back = w.back()
    #expect(back && !w.forward && w.page == 0)
}

@Test("number keys pick the first nine options")
func numberKeysPick() {
    let w = wizard([question("Which?", 10, true)])
    #expect(w.keyOption(1) == 0 && w.keyOption(9) == 8)
    #expect(w.keyOption(10) == nil && w.keyOption(0) == nil)
    #expect(wizard([question("Which?", 3, true)]).keyOption(5) == nil)
}

@Test("a pick over typed words clears them and the picks they hid")
func aPickOverTypedWords() {
    var w = wizard([question("Which?", 3, true)])
    _ = w.select(0)
    w.setTyped("foo")
    _ = w.select(0)
    #expect(w.currentTyped == "")
    #expect(w.isPicked(0))
    w.setTyped("bar")
    _ = w.select(1)
    #expect(w.isPicked(1) && !w.isPicked(0))
}

@Test("answers are keyed by question text")
func answersAreKeyedByQuestionText() {
    var w = wizard([question("Which files?", 3, true), question("Why?", 2, false)])
    _ = w.select(0)
    _ = w.select(2)
    _ = w.advance()
    _ = w.select(0)
    w.setTyped("  because  ")
    guard case .done(let answers) = w.advance() else { Issue.record("the last page submits"); return }
    #expect(answers["Which files?"] == "o1, o3")
    #expect(answers["Why?"] == "because")
}
