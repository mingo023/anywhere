import Testing
@testable import ChatModel

private let sample: [Row] = [.prompt(turn: 0), .fold(turn: 0), .end(turn: 0), .prompt(turn: 1), .fold(turn: 1), .live(turn: 1)]

@Test("a prompt sticks once its turn scrolls past its top")
func aPromptSticks() {
    #expect(Sticky.stuck(sample, top: 0, offset: 0) == nil)
    #expect(Sticky.stuck(sample, top: 0, offset: 12) == 0)
    #expect(Sticky.stuck(sample, top: 2, offset: 0) == 0)
    #expect(Sticky.stuck(sample, top: 3, offset: 0) == nil)
    #expect(Sticky.stuck(sample, top: 5, offset: 40) == 3)
}

@Test("a send held at the top before its echo sticks nothing")
func aSendHeldAtTheTop() {
    let rows: [Row] = [.prompt(turn: 0), .fold(turn: 0), .end(turn: 0), .pending(0), .working]
    #expect(Sticky.stuck(rows, top: 3, offset: 0) == nil)
    #expect(Sticky.stuck(rows, top: 4, offset: 10) == nil)
}

@Test("nothing sticks above the first prompt")
func nothingSticksAboveTheFirstPrompt() {
    let rows: [Row] = [.working, .prompt(turn: 0)]
    #expect(Sticky.stuck(rows, top: 0, offset: 30) == nil)
    #expect(Sticky.stuck([], top: 0, offset: 30) == nil)
    #expect(Sticky.stuck(rows, top: 9, offset: 0) == 1)
}

@Test("at its end the top row is the first that reaches the top")
func atItsEndTheTopRow() {
    let heights: [Double] = [100, 300, 50]
    let from = { (ix: Int) in heights[ix...].reduce(0, +) }
    #expect(Sticky.topAtEnd(3, shown: 200, from: from) == (1, 150))
    #expect(Sticky.topAtEnd(3, shown: 350, from: from) == (1, 0))
    #expect(Sticky.topAtEnd(3, shown: 900, from: from) == (0, 0))
}

@Test("the next prompt or a pending send pushes the card")
func theNextPromptPushes() {
    #expect(Sticky.nextPrompt(sample, top: 1) == 3)
    #expect(Sticky.nextPrompt(sample, top: 3) == nil)
    #expect(Sticky.nextPrompt([.prompt(turn: 0), .live(turn: 0), .pending(0)], top: 0) == 2)
}

@Test("the card moves up only once the next prompt reaches it")
func theCardMovesUp() {
    #expect(Sticky.pushed(bottom: 80, next: nil) == 0)
    #expect(Sticky.pushed(bottom: 80, next: 200) == 0)
    #expect(Sticky.pushed(bottom: 80, next: 50) == -30)
}

@Test("a pushed card loses its shadow over its first 12px")
func aPushedCardLosesItsShadow() {
    #expect(Sticky.lift(0) == 1)
    #expect(Sticky.lift(-6) == 0.5)
    #expect(Sticky.lift(-30) == 0)
}

@Test("a folded card is one line tall and an opened one grows to 160px")
func aFoldedCardIsOneLineTall() {
    #expect(Sticky.height(lines: 9, open: false) == Sticky.height)
    #expect(Sticky.height(lines: 1, open: true) == Sticky.openTop + Sticky.knob + Sticky.openBottom)
    #expect(Sticky.height(lines: 50, open: true) == Sticky.openTop + 1 + Sticky.openMaxHeight + Sticky.openBottom)
}

@Test("a folded prompt reads its line breaks as spaces")
func aFoldedPromptReadsLineBreaksAsSpaces() {
    #expect(Sticky.oneLine("Fix these:\n1) filter\n\n  2) sort ") == "Fix these: 1) filter 2) sort")
}

@Test("a prompt of only images counts them")
func aPromptOfOnlyImagesCountsThem() {
    #expect(Sticky.imageCount(1) == "1 image")
    #expect(Sticky.imageCount(4) == "4 images")
}

@Test("a prompt lists its images under its text")
func aPromptListsItsImages() {
    let block = "# Attached images\n\n- /h/attachments/a.png\n- /h/attachments/b.png"
    let images = ["/h/attachments/a.png", "/h/attachments/b.png"]
    #expect(prompt(" Fix CI ", images: images) == "Fix CI\n\n\(block)")
    #expect(prompt("", images: images) == block)
    #expect(prompt(" Fix CI ", images: []) == "Fix CI")
}

@Test("a sent prompt splits back into its text and image paths")
func aSentPromptSplitsBack() {
    let images = ["/h/a.png", "/h/b c.png"]
    #expect(splitImages(prompt("Fix CI", images: images)) == ("Fix CI", images))
    #expect(splitImages(prompt("", images: images)) == ("", images))
    #expect(splitImages("Fix CI") == ("Fix CI", []))
    let typed = "Explain\n\n# Attached images\n\nthis heading"
    #expect(splitImages(typed) == (typed, []))
}
