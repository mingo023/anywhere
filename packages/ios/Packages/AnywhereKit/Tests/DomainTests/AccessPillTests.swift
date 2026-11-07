import Testing
@testable import Domain

@Test("a phone sees the desktop's accesses, those pocketd won't let it pick locked")
func aPhoneSeesEveryAccessWithThosePastItsCeilingLocked() {
    let switching = AccessPill.choices(max: "edits", launching: false)
    #expect(switching.map(\.value) == ["ask", "edits", "auto", "full"])
    #expect(switching.map { $0.locked == nil } == [true, true, false, false])
    let launching = AccessPill.choices(max: "auto", launching: true)
    #expect(launching.map(\.value) == ["settings", "ask", "edits", "auto", "full"])
    #expect(launching.map { $0.locked == nil } == [false, true, true, true, false])
    #expect(AccessPill.label("auto") == "Auto" && AccessPill.label("") == "Agent's settings")
}

@Test("a phone switches to Full once the Mac raised its ceiling to Full")
func aPhoneSwitchesToFullOnceTheMacAllowsIt() {
    #expect(AccessPill.choices(max: "full", launching: false).map { $0.locked == nil } == [true, true, true, true])
    #expect(AccessPill.choices(max: "auto", launching: false).last?.locked == "Raise it on your Mac: ⌘K → Phone access level")
    #expect(AccessPill.choices(max: "full", launching: true).first?.locked == "Only from your Mac")
}

@Test("a phone never starts an agent on Full, even under a Full ceiling")
func aPhoneNeverStartsOnFull() {
    #expect(AccessPill.choices(max: "full", launching: true).map { $0.locked == nil } == [false, true, true, true, false])
    #expect(AccessPill.choices(max: "full", launching: true).last?.locked == "Only from your Mac")
}

@Test("only a switch into Full asks first")
func onlyASwitchIntoFullAsksFirst() {
    #expect(AccessPill.confirms("full", from: "edits"))
    #expect(!AccessPill.confirms("full", from: "full"))
    #expect(!AccessPill.confirms("auto", from: "ask"))
}
