import Testing
@testable import Domain

@Test("a phone is offered every access the desktop offers, its own settings only at launch")
func aPhoneIsOfferedEveryAccess() {
    #expect(AccessPill.choices(launching: false).map(\.value) == ["ask", "edits", "auto", "full"])
    #expect(AccessPill.choices(launching: true).map(\.value) == ["settings", "ask", "edits", "auto", "full"])
    #expect(AccessPill.label("auto") == "Auto" && AccessPill.label("") == "Agent's settings")
}

@Test("only a switch into Full asks first")
func onlyASwitchIntoFullAsksFirst() {
    #expect(AccessPill.confirms("full", from: "edits"))
    #expect(!AccessPill.confirms("full", from: "full"))
    #expect(!AccessPill.confirms("auto", from: "ask"))
}
