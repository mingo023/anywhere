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
