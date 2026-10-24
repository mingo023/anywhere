import Foundation
import Testing
@testable import ActivityModel

@Suite("Mac activity")
struct ActivityModelTests {
    @Test("the content state pocketd pushes decodes, with its time as Unix seconds")
    func decodes() throws {
        let json = #"{"working":2,"needsYou":1,"unseen":3,"top":{"slot":"a1","status":"needsYou","sinceUnix":1792832400.5,"verb":"Bash","subject":"rm -rf build","answerable":true,"request":"r1"}}"#
        let state = try JSONDecoder().decode(MacActivity.ContentState.self, from: Data(json.utf8))
        #expect(state.needsYou == 1)
        #expect(state.top?.request == "r1")
        #expect(state.top?.since == Date(timeIntervalSince1970: 1_792_832_400.5))
    }

    @Test("a state with nothing on top leaves it out")
    func noTop() throws {
        let state = MacActivity.ContentState(working: 0, needsYou: 0, unseen: 0)
        let json = try JSONDecoder().decode([String: Int].self, from: JSONEncoder().encode(state))
        #expect(json == ["working": 0, "needsYou": 0, "unseen": 0])
    }
}
