import Foundation
import Testing
@testable import Domain

@Suite("Routes")
struct RouteTests {
    @Test("a file route opens at its line, and at the top without one")
    func fileLine() {
        #expect(Route.file(mac: "m1", tree: "/r", path: "/r/a.rs") == .file(mac: "m1", tree: "/r", path: "/r/a.rs", line: nil))
        #expect(Route.file(mac: "m1", tree: "/r", path: "/r/a.rs", line: 842).mac == "m1")
    }

    @Test("a new session route carries its place and prompt, and neither by default")
    func newSession() {
        #expect(Route.newSession(mac: "m1", project: "/r") == .newSession(mac: "m1", project: "/r", place: nil, prompt: nil))
        let route = Route.newSession(mac: "m1", project: "/r", place: "/r/.worktrees/calm-otter", prompt: "Fix the build")
        guard case .newSession(_, _, let place, let prompt) = route else { Issue.record("not a new session"); return }
        #expect(place == "/r/.worktrees/calm-otter")
        #expect(prompt == "Fix the build")
    }

    @Test("stacks saved before routes had a line, place, prompt or chat still restore", arguments: [
        (#"{"file":{"mac":"m1","tree":"/r","path":"/r/a.rs"}}"#, Route.file(mac: "m1", tree: "/r", path: "/r/a.rs")),
        (#"{"newSession":{"mac":"m1","project":"/r"}}"#, Route.newSession(mac: "m1", project: "/r")),
        (#"{"files":{"mac":"m1","tree":"/r","dir":""}}"#, Route.files(mac: "m1", tree: "/r", dir: "")),
    ])
    func restores(_ json: String, _ route: Route) throws {
        #expect(try JSONDecoder().decode(Route.self, from: Data(json.utf8)) == route)
    }

    @Test("a route survives saving and restoring with every value set")
    func roundTrips() throws {
        let routes: [Route] = [
            .file(mac: "m1", tree: "/r", path: "/r/a.rs", line: 7, chat: "a1"),
            .files(mac: "m1", tree: "/r", dir: "src", chat: "a1"),
            .newSession(mac: "m1", project: "/r", place: "local-1", prompt: "Explain lines 40–58"),
        ]
        #expect(try JSONDecoder().decode([Route].self, from: JSONEncoder().encode(routes)) == routes)
    }
}
