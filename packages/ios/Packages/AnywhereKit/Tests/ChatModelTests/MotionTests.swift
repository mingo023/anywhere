import Testing
import Wire
@testable import ChatModel

private func words(_ n: Int) -> String { String(repeating: "word ", count: n) }

@Test("a chunk landing at once shows word by word within a second and a half")
func aChunkShowsWordByWord() {
    let text = words(128)
    var r = Reveal.shown(0, 0)
    r.arrive(text.utf8.count, 0)
    #expect(r.cut(text, 0) == 0)
    let half = r.cut(text, 0.7)
    #expect((300..<340).contains(half))
    #expect(String(text.utf8.dropFirst(half - 4).prefix(4))! == "word")
    #expect(r.cut(text, 1.4) == text.utf8.count)
}

@Test("a few words still take the quickest reveal")
func aFewWordsTakeTheQuickest() {
    let text = words(2)
    var r = Reveal.shown(0, 0)
    r.arrive(text.utf8.count, 0)
    #expect(r.cut(text, 0.1) == 4)
    #expect(r.cut(text, 0.3) == text.utf8.count)
}

@Test("text arriving mid reveal carries on from what shows")
func textArrivingMidReveal() {
    let text = words(128)
    var r = Reveal.shown(0, 0)
    r.arrive(320, 0)
    let before = r.cut(text, 0.5)
    r.arrive(text.utf8.count, 0.5)
    #expect(r.cut(text, 0.5) == before)
    #expect(r.cut(text, 1.9) == text.utf8.count)
}

@Test("a reveal moves until its last word has faded in")
func aRevealMovesUntilFaded() {
    var r = Reveal.shown(0, 0)
    #expect(!r.moving(Reveal.fade))
    r.arrive(10, 0)
    #expect(r.moving(0.3 + Reveal.fade - 0.001))
    #expect(!r.moving(0.3 + Reveal.fade))
}

@Test("text already there or rewritten shows at once")
func textAlreadyThereShowsAtOnce() {
    let text = "héllo wörld, ünïcode"
    #expect(Reveal.shown(text.utf8.count, 0).cut(text, 0) == text.utf8.count)
    var r = Reveal.shown(0, 0)
    r.arrive(text.utf8.count, 0)
    r.arrive(5, 0.01)
    #expect(r.cut(String(text.utf8.prefix(5))!, 0.01) == 5)
}

@Test("a cut inside a wide character still ends on a word")
func aCutInsideAWideCharacter() {
    let text = "héllo wörld"
    var r = Reveal.shown(0, 0)
    r.arrive(text.utf8.count, 0)
    for n in 0...300 {
        let cut = r.cut(text, Double(n) / 1000)
        #expect([0, 6, text.utf8.count].contains(cut), "\(n)ms: \(cut)")
    }
}

@Test("a later block of a message shows only once the reveal reaches it")
func aLaterBlockShowsOnceReached() {
    let first = words(64)
    let second = words(64)
    var r = Reveal.shown(0, 0)
    r.arrive((first + second).utf8.count, 0)
    #expect(r.cut(second, from: first.utf8.count, 0.5) == 0)
    let part = r.cut(second, from: first.utf8.count, 0.9)
    #expect(part > 0 && part < second.utf8.count)
    #expect(r.cut(second, from: first.utf8.count, 1.4) == second.utf8.count)
}

@Test("the jump pill shows past 320px and hides at two")
func theJumpPillShows() {
    #expect(!jumpVisibility(wasShown: false, distance: 320))
    #expect(jumpVisibility(wasShown: false, distance: 321))
    #expect(jumpVisibility(wasShown: true, distance: 100))
    #expect(!jumpVisibility(wasShown: true, distance: 2))
}

@Test("a glide eases the whole distance and a far end jumps")
func aGlideEasesTheWholeDistance() {
    #expect(Glide(distance: 2501, viewport: 1000) == nil)
    var glide = Glide(distance: 1000, viewport: 1000)!
    let (first, done) = glide.step(0.06)
    #expect(!done && first > 500)
    let (rest, end) = glide.step(0.4)
    #expect(end)
    #expect(abs(first + rest - 1000) < 0.01)
}

@Test("a prompt with nothing before it holds at the top as soon as its row is measured")
func aPromptWithNothingBefore() {
    var pin = Pin()
    let first = pin.step(viewport: 800, above: 0, below: nil, glide: nil)
    #expect(first && pin.spacer == 0 && pin.unmeasured)
    let second = pin.step(viewport: 800, above: 0, below: 100, glide: nil)
    #expect(second)
    #expect(pin.spacer == 668 && pin.anchored && !pin.unmeasured)
}

@Test("the prompt holds once its row is laid out though the reply under it is still measuring")
func thePromptHoldsOnceLaidOut() {
    var pin = Pin()
    #expect(pin.below(measured: nil, laidOut: nil) == nil)
    let below = pin.below(measured: nil, laidOut: 100)
    let held = pin.step(viewport: 800, above: 0, below: below, glide: nil)
    #expect(held)
    #expect(pin.spacer == 668 && pin.anchored && !pin.unmeasured)
    #expect(pin.below(measured: nil, laidOut: 300) == nil)
    #expect(pin.below(measured: 120, laidOut: 300) == 120)
}

@Test("the room shrinks as the reply grows and the pin lets go once it fills the viewport")
func theRoomShrinks() {
    var pin = Pin()
    _ = pin.step(viewport: 800, above: 0, below: 100, glide: nil)
    _ = pin.step(viewport: 800, above: 0, below: 500, glide: nil)
    #expect(pin.spacer == 268)
    let held = pin.step(viewport: 800, above: 0, below: 768, glide: nil)
    #expect(!held)
}

@Test("while a row under the prompt is measured the spacer fills the viewport")
func whileARowIsMeasured() {
    var pin = Pin()
    _ = pin.step(viewport: 800, above: 0, below: 500, glide: nil)
    let held = pin.step(viewport: 800, above: 0, below: nil, glide: nil)
    #expect(held)
    #expect(pin.spacer == 767)
    #expect(Pin.top + Pin.bottom + pin.spacer < 800)
    _ = pin.step(viewport: 800, above: 0, below: 100, glide: nil)
    #expect(pin.spacer == 668)
}

@Test("a scroll holds the prompt only from the end")
func aScrollHoldsOnlyFromTheEnd() {
    var pin = Pin()
    _ = pin.step(viewport: 800, above: 0, below: 100, glide: nil)
    pin.moved(atEnd: false)
    _ = pin.step(viewport: 800, above: 0, below: 100, glide: nil)
    #expect(pin.spacer == 668 && !pin.anchored)
    pin.moved(atEnd: true)
    #expect(pin.anchored)
}

@Test("a steer leaves the pin on the prompt it steers")
func aSteerLeavesThePin() {
    var pin = Pin()
    let rows: [Row] = [.prompt(turn: 0), .fold(turn: 0), .live(turn: 0), .pending(0), .working]
    pin.steered()
    #expect(pin.row(rows) == 0)
    let echoed: [Row] = [.prompt(turn: 0), .fold(turn: 0), .end(turn: 0), .prompt(turn: 1), .live(turn: 1)]
    #expect(pin.row(echoed) == 0)
    #expect(Pin().row(echoed) == 3)
}

@Test("the pin holds the prompt last sent")
func thePinHoldsThePromptLastSent() {
    let rows: [Row] = [.prompt(turn: 0), .end(turn: 0), .prompt(turn: 1), .fold(turn: 1)]
    #expect(Pin().row(rows) == 2)
    #expect(Pin().row([.prompt(turn: 0), .pending(0), .working]) == 1)
    #expect(Pin().row([]) == nil)
}

@Test("a prompt holds a quarter down under what came before it")
func aPromptHoldsAQuarterDown() {
    var pin = Pin()
    let held = pin.step(viewport: 800, above: 1000, below: 100, glide: nil)
    #expect(held)
    let top = pin.top(0)
    #expect(top == (200, false) && pin.spacer == 476)
    let full = pin.step(viewport: 800, above: 1000, below: 576, glide: nil)
    #expect(!full)
}

@Test("a prompt with little before it holds just under that")
func aPromptWithLittleBefore() {
    var pin = Pin()
    _ = pin.step(viewport: 800, above: 60, below: 100, glide: nil)
    let top = pin.top(0)
    #expect(top == (68, false))
}

@Test("the chat glides a sent prompt from the end to where its held")
func theChatGlidesASentPrompt() {
    var pin = Pin()
    _ = pin.step(viewport: 800, above: 1000, below: 100, glide: 0)
    let start = pin.top(0)
    #expect(start == (676, true))
    let (half, gliding) = pin.top(Pin.glide / 2)
    #expect(gliding && half < 676 - (676 - 200) / 2)
    let end = pin.top(Pin.glide)
    #expect(end == (200, false))
    let after = pin.top(Pin.glide * 2)
    #expect(after == (200, false))
}

@Test("a scroll stops the glide")
func aScrollStopsTheGlide() {
    var pin = Pin()
    _ = pin.step(viewport: 800, above: 1000, below: 100, glide: 0)
    pin.moved(atEnd: true)
    let top = pin.top(0)
    #expect(top == (200, false))
}

@Test("a block that ended before the reveal began shows whole for all of it")
func aBlockBeforeTheRevealIsWhole() {
    let first = words(10)
    var r = Reveal.shown(first.utf8.count, 0)
    r.arrive(first.utf8.count + 40, 0)
    #expect(!r.reaches(before: first.utf8.count))
    #expect(r.cut(first, 0) == first.utf8.count)
    #expect(r.reaches(before: first.utf8.count + 1))
}
